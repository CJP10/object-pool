use std::{
    cell::UnsafeCell,
    iter::FromIterator,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::atomic::{
        AtomicU64,
        Ordering::{Acquire, Relaxed, Release},
    },
};

const U64_BITS: usize = u64::BITS as usize;

pub struct Pool<T> {
    objects: Box<[UnsafeCell<Option<T>>]>,
    bitset: AtomicBitSet,
}

impl<A> FromIterator<A> for Pool<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let objects = iter
            .into_iter()
            .map(|o| UnsafeCell::new(Some(o)))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            bitset: AtomicBitSet::new(objects.len()),
            objects,
        }
    }
}

impl<T> Pool<T> {
    pub fn pull(&self) -> Option<Reusable<T>> {
        unsafe {
            self.bitset.zero_first_set_bit().map(|index| Reusable {
                pool: &self,
                value: ManuallyDrop::new(
                    self.objects[index]
                        .get()
                        .replace(None)
                        .expect("Object should not be null"),
                ),
                index,
            })
        }
    }

    pub fn len(&self) -> usize {
        let mut len = 0;
        for int in self.bitset.ints.iter() {
            len += int.load(Relaxed).count_ones() as usize
        }
        len
    }

    pub fn capacity(&self) -> usize {
        self.objects.len()
    }

    fn ret(&self, index: usize, value: T) {
        unsafe {
            let old = self.objects[index].get().replace(Some(value));
            debug_assert!(old.is_none())
        }
        self.bitset.set(index)
    }
}

pub struct Reusable<'a, T> {
    pool: &'a Pool<T>,
    value: ManuallyDrop<T>,
    index: usize,
}

impl<'a, T> Deref for Reusable<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, T> DerefMut for Reusable<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<'a, T> Drop for Reusable<'a, T> {
    fn drop(&mut self) {
        unsafe {
            self.pool
                .ret(self.index, ManuallyDrop::take(&mut self.value))
        }
    }
}

struct AtomicBitSet {
    ints: Box<[AtomicU64]>,
}

impl AtomicBitSet {
    fn new(num_of_bits: usize) -> Self {
        let num_of_ints = ((num_of_bits + U64_BITS - 1) / U64_BITS).max(1);
        let mut bits: Vec<AtomicU64> = (1..num_of_ints).map(|_| AtomicU64::new(u64::MAX)).collect();
        bits.push(AtomicU64::new(
            u64::MAX
                .checked_shr((U64_BITS - num_of_bits % U64_BITS) as u32)
                .unwrap_or(0),
        ));
        Self {
            ints: bits.into_boxed_slice(),
        }
    }

    fn zero_first_set_bit(&self) -> Option<usize> {
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

    fn set(&self, index: usize) {
        let int = index / U64_BITS;
        let bit = index % U64_BITS;
        let bitmap = self.ints[int].fetch_or(1 << bit, Release);
        debug_assert!(bitmap & 1 << bit == 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::experimental::Pool;
    use std::iter::FromIterator;
    use std::sync::atomic::Ordering::Relaxed;

    #[test]
    fn empty() {
        let p: Pool<()> = std::iter::empty().collect();
        assert_eq!(p.len(), 0);
        assert_eq!(p.capacity(), 0);
        assert_eq!(p.bitset.ints.len(), 1);
        assert_eq!(p.bitset.ints[0].load(Relaxed), 0);
        assert!(p.pull().is_none());
    }

    #[test]
    fn pull_set_return() {
        let p: Pool<usize> = (0..100usize).collect();
        assert_eq!(p.len(), 100);
        assert_eq!(p.capacity(), 100);
        assert_eq!(p.bitset.ints.len(), 2);
        assert_eq!(p.bitset.ints[0].load(Relaxed), u64::MAX);
        assert_eq!(p.bitset.ints[1].load(Relaxed), u64::MAX >> 28);

        let mut objects = Vec::new();
        for _ in 0..p.len() {
            let mut o = p.pull();
            if let Some(ref mut o) = o {
                **o += 1;
            }
            objects.push(o)
        }

        assert!(p
            .bitset
            .ints
            .iter()
            .map(|x| x.load(Relaxed))
            .all(|x| x == 0));

        drop(objects);

        assert_eq!(p.bitset.ints[0].load(Relaxed), u64::MAX);
        assert_eq!(p.bitset.ints[1].load(Relaxed), u64::MAX >> 28);
        unsafe {
            assert!(p
                .objects
                .into_iter()
                .enumerate()
                .map(|(i, x)| (i, (*x.get())))
                .all(|(i, x)| x.is_some() && i + 1 == x.unwrap()));
        }
    }
}
