//! A thread-safe object pool with automatic return and attach/detach semantics
//!
//! The goal of an object pool is to reuse expensive to allocate objects or frequently allocated objects
//!
//! Common use case is when using buffer to read IO.
//! You would create a Pool of size n, containing Vec<u8> that can be used to call something like `file.read_to_end(buff)`
//!
//! ## Warning
//!
//! Objects in the pool are not automatically reset, they are returned but NOT reset
//! You may want to call `object.reset()` or  `object.clear()`
//! or any other equivalent for the object you are using after pulling from the pool
//!
//! # Examples
//!
//! ## Creating a Pool
//!
//! The general pool creation looks like this
//! ```
//!  let pool: Pool<T> = Pool::new(capacity, || T::new());
//! ```
//! Creating a pool with 32 `Vec<u8>` with capacity of 4096
//! ```
//!  let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096);
//! ```
//!
//! ## Using a Pool
//!
//! Basic usage for pulling from the pool
//! ```
//! let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096);
//! let mut reusable_buff = pool.pull().unwrap(); // returns None when the pool is saturated
//! reusable_buff.clear(); //clear the buff before using
//! some_file.read_to_end(reusable_buff.deref_mut());
//! //reusable_buff is automatically returned to the pool when it goes out of scope
//! ```
//! Pull from poll and `detach()`
//! ```
//! let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096);
//! let mut reusable_buff = pool.pull().unwrap(); // returns None when the pool is saturated
//! reusable_buff.clear(); //clear the buff before using
//! let s = String::from(reusable_buff.detach());
//! s.push_str("hello, world!");
//! reusable_buff.attach(s.into_bytes()); //need to reattach the buffer before reusable goes out of scope
//! //reusable_buff is automatically returned to the pool when it goes out of scope
//! ```

#![feature(alloc)]
#![feature(raw_vec_internals)]
#![feature(test)]

extern crate alloc;
extern crate test;

use alloc::raw_vec::RawVec;
use std::ops::{
    Deref, DerefMut, Drop,
};
use std::ptr;
use std::sync::atomic::{
    AtomicBool, Ordering,
};

pub struct Pool<T> {
    inner: RawVec<Entry<T>>
}

impl<T> Pool<T> {
    pub fn new<F>(cap: usize, init: F) -> Pool<T>
        where F: Fn() -> T {
        let inner = RawVec::with_capacity_zeroed(cap);

        for i in 0..cap {
            unsafe {
                let raw_ptr: *mut Entry<T> = inner.ptr();
                raw_ptr.offset(i as isize).write(Entry {
                    data: init(),
                    locked: AtomicBool::new(false),
                });
            }
        }

        Pool {
            inner
        }
    }

    pub fn pull(&self) -> Option<Reusable<T>> {
        for i in 0..self.inner.cap() {
            let raw_ptr: *mut Entry<T> = unsafe { self.inner.ptr().offset(i as isize) };
            let entry = unsafe { raw_ptr.as_mut() }.unwrap();

            if !entry.locked.compare_and_swap(false, true, Ordering::AcqRel) {
                return Some(Reusable {
                    entry: raw_ptr,
                    index: i,
                    detached: false,
                });
            }
        }

        return None;
    }
}

//for testing only
impl Pool<Vec<u8>> {
    pub fn count(&self) -> u64 {
        let mut count = 0 as u64;

        for i in 0..self.inner.cap() {
            let raw_ptr: *mut Entry<Vec<u8>> = unsafe { self.inner.ptr().offset(i as isize) };
            let entry = unsafe { raw_ptr.as_mut() }.unwrap();

            count += entry.data.len() as u64;
        }

        return count;
    }
}

struct Entry<T> {
    data: T,
    locked: AtomicBool,
}

pub struct Reusable<T> {
    entry: *mut Entry<T>,
    detached: bool,
    index: usize,
}

impl<T> Reusable<T> {
    pub fn detach(&mut self) -> T {
        if self.detached {
            panic!("double detach not allowed")
        }

        self.detached = true;
        let copy_entry = unsafe { ptr::read(self.entry) };
        return copy_entry.data;
    }

    pub fn attach(&mut self, value: T) {
        self.detached = false;
        unsafe {
            ptr::write(self.entry, Entry {
                data: value,
                locked: AtomicBool::new(true),
            });
        }
    }
}

impl<T> Drop for Reusable<T> {
    fn drop(&mut self) {
        if self.detached {
            panic!("reusable dropped while detached")
        }

        let entry = unsafe { self.entry.as_mut() }.unwrap();

        let mut timeout = 0;
        while !entry.locked.compare_and_swap(true, false, Ordering::AcqRel) {
            if timeout > 1_000_000_000 {
                panic!("timed out dropping reusable {}", self.index)
            }

            timeout += 1;
        }
    }
}

impl<T> Deref for Reusable<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        return unsafe { &self.entry.as_ref().unwrap().data };
    }
}


impl<T> DerefMut for Reusable<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        return unsafe { &mut self.entry.as_mut().unwrap().data };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use test::Bencher;

    use super::*;

    #[test]
    fn round_trip() {
        let pool: Arc<Pool<Vec<u8>>> = Arc::new(Pool::new(10, || Vec::with_capacity(1)));

        for t in 0..10 {
            let tmp = pool.clone();
            std::thread::spawn(move || {
                for i in 0..1000000 {
                    let mut reusable = tmp.pull().unwrap();
                    if i % 2 == 0 {
                        let mut vec = reusable.detach();
                        vec.push(i as u8);
                        reusable.attach(vec);
                    } else {
                        reusable.push(i as u8);
                    }
                }
            });
        }

        //wait for everything to finish
        std::thread::sleep_ms(3000);

        assert_eq!(pool.count(), 10000000)
    }

    #[bench]
    fn bench_pull(b: &mut Bencher) {
        let pool: Arc<Pool<Vec<u8>>> = Arc::new(Pool::new(10, || Vec::with_capacity(100000000)));

        b.iter(|| {
            pool.pull().unwrap()
        });
    }

    #[bench]
    fn bench_pull_detach(b: &mut Bencher) {
        let pool: Arc<Pool<Vec<u8>>> = Arc::new(Pool::new(10, || Vec::with_capacity(100000000)));

        b.iter(|| {
            let mut reusable = pool.pull().unwrap();
            let item = reusable.detach();
            reusable.attach(item);
            reusable
        });
    }
}