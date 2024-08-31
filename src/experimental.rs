use std::{
    cell::UnsafeCell,
    iter::FromIterator,
    mem::{ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    sync::atomic::Ordering::{Acquire, Relaxed, Release},
};

#[cfg(not(loom))]
use std::sync::{atomic::AtomicU64, Arc};

#[cfg(loom)]
use loom::sync::{atomic::AtomicU64, Arc};

const U64_BITS: usize = u64::BITS as usize;

pub struct Pool<T> {
    objects: Box<[UnsafeCell<MaybeUninit<T>>]>,
    freelist: FreeList,
}

impl<A> FromIterator<A> for Pool<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let objects = iter
            .into_iter()
            .map(|o| UnsafeCell::new(MaybeUninit::new(o)))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            freelist: FreeList::new(objects.len()),
            objects,
        }
    }
}

impl<T> Pool<T> {
    pub fn pull(&self) -> Option<ObjectRef<T>> {
        unsafe {
            self.freelist.first_free().map(|index| ObjectRef {
                pool: &self,
                value: (*self.objects[index].get()).assume_init_mut(),
                index,
            })
        }
    }

    #[cfg(not(loom))]
    pub fn pull_owned(self: &Arc<Self>) -> Option<Object<T>> {
        unsafe {
            self.freelist.first_free().map(|index| Object {
                pool: Arc::clone(self),
                value: ManuallyDrop::new(
                    self.objects[index]
                        .get()
                        .replace(MaybeUninit::uninit())
                        .assume_init(),
                ),
                index,
            })
        }
    }

    pub fn len(&self) -> usize {
        let mut len = 0;
        for int in self.freelist.ints.iter() {
            len += int.load(Relaxed).count_ones() as usize
        }
        len
    }

    pub fn capacity(&self) -> usize {
        self.objects.len()
    }
}

impl<T> Drop for Pool<T> {
    fn drop(&mut self) {
        for (i, int) in self.freelist.ints.iter().enumerate() {
            let mut bits = int.load(Acquire);
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                unsafe { (*self.objects[i * U64_BITS + bit].get()).assume_init_drop() }
                bits &= !(1 << bit);
            }
        }
    }
}

pub struct ObjectRef<'a, T> {
    pool: &'a Pool<T>,
    value: &'a mut T,
    index: usize,
}

impl<'a, T> Deref for ObjectRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value as _
    }
}

impl<'a, T> DerefMut for ObjectRef<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<'a, T> Drop for ObjectRef<'a, T> {
    fn drop(&mut self) {
        self.pool.freelist.free(self.index);
    }
}

pub struct Object<T> {
    pool: Arc<Pool<T>>,
    value: ManuallyDrop<T>,
    index: usize,
}

impl<T> Deref for Object<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for Object<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> Drop for Object<T> {
    fn drop(&mut self) {
        unsafe {
            self.pool.objects[self.index]
                .get()
                .write(MaybeUninit::new(ManuallyDrop::take(&mut self.value)));
        }
        self.pool.freelist.free(self.index)
    }
}

struct FreeList {
    ints: Box<[AtomicU64]>,
}

impl FreeList {
    fn new(entries: usize) -> Self {
        let mut bits: Vec<AtomicU64> = (0..entries)
            .step_by(U64_BITS)
            .map(|_| AtomicU64::new(u64::MAX))
            .collect();

        let out_of_bounds_bits = U64_BITS - (entries % U64_BITS);
        if bits.is_empty() {
            bits = vec![AtomicU64::new(0)];
        } else if out_of_bounds_bits != 0 {
            *bits.last_mut().unwrap() = AtomicU64::new(u64::MAX >> out_of_bounds_bits);
        }

        Self {
            ints: bits.into_boxed_slice(),
        }
    }

    fn first_free(&self) -> Option<usize> {
        for (i, int) in self.ints.iter().enumerate() {
            let mut bits = int.load(Relaxed);
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                match int.compare_exchange_weak(bits, bits & !(1 << bit), Acquire, Relaxed) {
                    Ok(_) => return Some(i * U64_BITS + bit),
                    Err(new_bits) => bits = new_bits,
                }
            }
        }
        None
    }

    fn free(&self, index: usize) {
        let int = index / U64_BITS;
        let bit = index % U64_BITS;
        let bits = self.ints[int].fetch_or(1 << bit, Release);
        debug_assert_eq!(bits & 1 << bit, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::Pool;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::Ordering::Relaxed;

    #[test]
    fn size_check() {
        let p: Pool<()> = std::iter::empty().collect();
        assert_eq!(p.len(), 0);
        assert_eq!(p.capacity(), 0);
        assert_eq!(p.freelist.ints.len(), 1);
        assert_eq!(p.freelist.ints[0].load(Relaxed), 0);
        assert!(p.pull().is_none());

        let p: Pool<usize> = (0..1usize).collect();
        assert_eq!(p.len(), 1);
        assert_eq!(p.capacity(), 1);
        assert_eq!(p.freelist.ints.len(), 1);
        assert_eq!(p.freelist.ints[0].load(Relaxed), 1);
        assert!(p.pull().is_some());

        let p: Pool<usize> = (0..65usize).collect();
        assert_eq!(p.len(), 65);
        assert_eq!(p.capacity(), 65);
        assert_eq!(p.freelist.ints.len(), 2);
        assert_eq!(p.freelist.ints[0].load(Relaxed), u64::MAX);
        assert_eq!(p.freelist.ints[1].load(Relaxed), 1);
        assert!(p.pull().is_some());

        let p: Pool<usize> = (0..500usize).collect();
        assert_eq!(p.len(), 500);
        assert_eq!(p.capacity(), 500);
        assert_eq!(p.freelist.ints.len(), 8);
        assert!(p.freelist.ints[0..7]
            .iter()
            .map(|x| x.load(Relaxed))
            .all(|x| x == u64::MAX));
        assert_eq!(p.freelist.ints[7].load(Relaxed), u64::MAX >> 12);
        assert!(p.pull().is_some());
    }

    #[test]
    fn full_and_partial_drop() {
        struct DropTest {
            drops: Rc<RefCell<Vec<bool>>>,
            index: usize,
        }

        impl Drop for DropTest {
            fn drop(&mut self) {
                self.drops.borrow_mut()[self.index] = true
            }
        }

        const N: usize = 500;
        let new_pool = || {
            let drops = Rc::new(RefCell::new(vec![false; N]));
            let p: Pool<DropTest> = (0..N)
                .map(|index| DropTest {
                    drops: drops.clone(),
                    index,
                })
                .collect();
            (drops, p)
        };

        let (drops, p) = new_pool();
        drop(p);
        assert!(drops.borrow().iter().all(|dropped| *dropped));

        let (drops, p) = new_pool();
        for _ in 0..N / 2 {
            std::mem::forget(p.pull().unwrap());
        }
        drop(p);

        assert!(drops.borrow()[..N / 2].iter().all(|dropped| !*dropped));
        assert!(drops.borrow()[N / 2..].iter().all(|dropped| *dropped));
    }

    #[test]
    fn pull_set_return() {
        let p: Pool<usize> = (0..100usize).collect();
        assert_eq!(p.len(), 100);
        assert_eq!(p.capacity(), 100);
        assert_eq!(p.freelist.ints.len(), 2);
        assert_eq!(p.freelist.ints[0].load(Relaxed), u64::MAX);
        assert_eq!(p.freelist.ints[1].load(Relaxed), u64::MAX >> 28);

        let mut objects = Vec::new();
        for _ in 0..p.len() {
            let mut o = p.pull();
            if let Some(ref mut o) = o {
                **o += 1;
            }
            objects.push(o)
        }

        assert!(p
            .freelist
            .ints
            .iter()
            .map(|x| x.load(Relaxed))
            .all(|x| x == 0));

        drop(objects);

        assert_eq!(p.freelist.ints[0].load(Relaxed), u64::MAX);
        assert_eq!(p.freelist.ints[1].load(Relaxed), u64::MAX >> 28);
        unsafe {
            assert!(p
                .objects
                .iter()
                .enumerate()
                .map(|(i, x)| (i, (*x.get()).assume_init_read()))
                .all(|(i, x)| i + 1 == x));
        }
    }
}

#[cfg(loom)]
mod loom_tests {
    use super::Pool;
    use loom::sync::Arc;

    #[test]
    fn concurrent_pull_mutate() {
        loom::model(|| {
            let p: Pool<Vec<_>> = (0..1).map(|_| vec![1, 2, 3]).collect();
            let p = Arc::new(p);
            let p1 = p.clone();

            let h = loom::thread::spawn(move || {
                if let Some(mut o) = p1.pull() {
                    let x = o.remove(0);
                    assert!(x == 1 || x == 2);
                    if x == 1 {
                        assert_eq!(o.as_slice(), &[2, 3]);
                    }
                    if x == 2 {
                        assert_eq!(o.as_slice(), &[3]);
                    }
                }
            });

            match p.pull() {
                Some(mut o) => {
                    assert!(o.len() == 2 || o.len() == 3);
                    if o.len() == 3 {
                        o.remove(0);
                        h.join().unwrap();
                    }
                    assert_eq!(o.as_slice(), &[2, 3]);
                }
                None => {
                    h.join().unwrap();
                    assert_eq!(p.pull().unwrap().as_slice(), &[2, 3]);
                }
            };
        });
    }
}
