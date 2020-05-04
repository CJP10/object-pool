//! A thread-safe object pool with automatic return and attach/detach semantics
//!
//! The goal of an object pool is to reuse expensive to allocate objects or frequently allocated objects
//!
//! # Examples
//!
//! ## Creating a Pool
//!
//! The general pool creation looks like this
//! ```
//!  let pool: Pool<T> = Pool::new(capacity, || T::new());
//! ```
//! Example pool with 32 `Vec<u8>` with capacity of 4096
//! ```
//!  let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096));
//! ```
//!
//! ## Using a Pool
//!
//! Basic usage for pulling from the pool
//! ```
//! let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096));
//! let mut reusable_buff = pool.pull().unwrap(); // returns None when the pool is saturated
//! reusable_buff.clear(); // clear the buff before using
//! some_file.read_to_end(reusable_buff);
//! // reusable_buff is automatically returned to the pool when it goes out of scope
//! ```
//! Pull from pool and `detach()`
//! ```
//! let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096));
//! let mut reusable_buff = pool.pull().unwrap(); // returns None when the pool is saturated
//! reusable_buff.clear(); // clear the buff before using
//! let (pool, reusable_buff) = reusable_buff.detach();
//! let mut s = String::from(reusable_buff);
//! s.push_str("hello, world!");
//! pool.attach(s.into_bytes()); // reattach the buffer before reusable goes out of scope
//! // reusable_buff is automatically returned to the pool when it goes out of scope
//! ```
//!
//! ## Using Across Threads
//!
//! You simply wrap the pool in a [`std::sync::Arc`]
//! ```
//! let pool: Arc<Pool<T>> = Arc::new(Pool::new(cap, || T::new()));
//! ```
//!
//! # Warning
//!
//! Objects in the pool are not automatically reset, they are returned but NOT reset
//! You may want to call `object.reset()` or  `object.clear()`
//! or any other equivalent for the object that you are using, after pulling from the pool
//!
//! [`std::sync::Arc`]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html

use parking_lot::Mutex;
use std::hint::unreachable_unchecked;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::ptr;

pub type Stack<T> = Vec<T>;

pub struct Pool<T> {
    objects: Mutex<Stack<T>>,
}

impl<T> Pool<T> {
    #[inline]
    pub fn new<F>(cap: usize, init: F) -> Pool<T>
    where
        F: Fn() -> T,
    {
        let mut objects = Stack::new();

        for _ in 0..cap {
            objects.push(init());
        }

        Pool {
            objects: Mutex::new(objects),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.objects.lock().len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.objects.lock().is_empty()
    }

    #[inline]
    pub fn try_pull(&self) -> Option<Reusable<T>> {
        self.objects
            .lock()
            .pop()
            .map(|data| Reusable::new(self, data))
    }

    #[inline]
    pub fn pull<F: Fn() -> T>(&self, fallback: F) -> Reusable<T> {
        self.try_pull()
            .unwrap_or_else(|| Reusable::new(self, fallback()))
    }

    #[inline]
    pub fn attach(&self, t: T) {
        self.objects.lock().push(t)
    }
}

pub struct Reusable<'a, T> {
    pool: &'a Pool<T>,
    data: Option<ManuallyDrop<T>>,
}

impl<'a, T> Reusable<'a, T> {
    #[inline]
    pub fn new(pool: &'a Pool<T>, t: T) -> Self {
        Self {
            pool,
            data: Some(ManuallyDrop::new(t)),
        }
    }

    #[inline]
    pub fn detach(mut self) -> (&'a Pool<T>, T) {
        unsafe {
            match self.data.take() {
                Some(data) => (self.pool, ManuallyDrop::into_inner(data)),
                None => unreachable_unchecked(),
            }
        }
    }
}

impl<'a, T> Deref for Reusable<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self.data {
            Some(ref data) => data,
            None => unsafe { unreachable_unchecked() },
        }
    }
}

impl<'a, T> DerefMut for Reusable<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self.data {
            Some(ref mut data) => data,
            None => unsafe { unreachable_unchecked() },
        }
    }
}

impl<'a, T> Drop for Reusable<'a, T> {
    #[inline]
    fn drop(&mut self) {
        if let Some(ref data) = self.data {
            unsafe { self.pool.attach(ManuallyDrop::into_inner(ptr::read(data))) }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Pool, Reusable};
    use std::mem::drop;

    #[test]
    fn detach() {
        let pool = Pool::new(1, || Vec::new());
        let (pool, mut object) = pool.try_pull().unwrap().detach();
        object.push(1);
        Reusable::new(&pool, object);
        assert_eq!(pool.try_pull().unwrap()[0], 1);
    }

    #[test]
    fn detach_then_attach() {
        let pool = Pool::new(1, || Vec::new());
        let (pool, mut object) = pool.try_pull().unwrap().detach();
        object.push(1);
        pool.attach(object);
        assert_eq!(pool.try_pull().unwrap()[0], 1);
    }

    #[test]
    fn pull() {
        let pool = Pool::<Vec<u8>>::new(1, || Vec::new());

        let object1 = pool.try_pull();
        let object2 = pool.try_pull();
        let object3 = pool.pull(|| Vec::new());

        assert!(object1.is_some());
        assert!(object2.is_none());
        drop(object1);
        drop(object2);
        drop(object3);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn e2e() {
        let pool = Pool::new(10, || Vec::new());
        let mut objects = Vec::new();

        for i in 0..10 {
            let mut object = pool.try_pull().unwrap();
            object.push(i);
            objects.push(object);
        }

        assert!(pool.try_pull().is_none());
        drop(objects);
        assert!(pool.try_pull().is_some());

        for i in 10..0 {
            let mut object = pool.objects.lock().pop().unwrap();
            assert_eq!(object.pop(), Some(i));
        }
    }
}
