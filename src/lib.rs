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
//! reusable_buff.clear(); //clear the buff before using
//! some_file.read_to_end(reusable_buff);
//! //reusable_buff is automatically returned to the pool when it goes out of scope
//! ```
//! Pull from poll and `detach()`
//! ```
//! let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096));
//! let mut reusable_buff = pool.pull().unwrap(); // returns None when the pool is saturated
//! reusable_buff.clear(); //clear the buff before using
//! let s = String::from(reusable_buff.detach(Vec::new()));
//! s.push_str("hello, world!");
//! reusable_buff.attach(s.into_bytes()); //reattach the buffer before reusable goes out of scope
//! //reusable_buff is automatically returned to the pool when it goes out of scope
//! ```
//!
//! ## Using Across Threads
//!
//! You simply wrap the pool in a [`std::sync::Arc`]
//! ```
//! let pool: Arc<Pool<T>> = Arc::new(Pool::new(cap, || T::new()));
//! ```
//!
//! [`std::sync::Arc`]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html

use std::mem;
use std::ops::{
    Deref, DerefMut,
};
use std::sync::{
    Mutex, MutexGuard,
};

pub struct Pool<T> {
    inner: Vec<Mutex<T>>
}

impl<T> Pool<T> {
    pub fn new<F>(cap: usize, init: F) -> Pool<T>
        where F: Fn() -> T {
        let mut inner = Vec::with_capacity(cap);

        for _ in 0..cap {
            inner.push(Mutex::new(init()));
        }

        Pool {
            inner
        }
    }

    pub fn pull(&self) -> Option<Reusable<T>> {
        for entry in &self.inner {
            let entry_guard = match entry.try_lock() {
                Ok(v) => v,
                Err(_) => { continue; }
            };

            return Some(Reusable {
                data: entry_guard,
            });
        }

        None
    }
}

//for testing only
impl Pool<Vec<u8>> {
    pub fn count(&self) -> u64 {
        let mut count = 0 as u64;

        for entry in &self.inner {
            let entry_guard = match entry.try_lock() {
                Ok(v) => v,
                Err(_) => { continue; }
            };

            count += entry_guard.len() as u64;
        }

        count
    }
}


pub struct Reusable<'a, T> {
    data: MutexGuard<'a, T>,
}

impl<'a, T> Reusable<'a, T> {
    pub fn detach(&mut self, replacement: T) -> T {
        mem::replace(&mut self.data, replacement)
    }

    pub fn attach(&mut self, data: T) -> T {
        mem::replace(&mut self.data, data)
    }
}

impl<'a, T> Deref for Reusable<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data.deref()
    }
}


impl<'a, T> DerefMut for Reusable<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data.deref_mut()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn round_trip() {
        let pool: Arc<Pool<Vec<u8>>> = Arc::new(Pool::new(10, || Vec::with_capacity(1)));

        for _ in 0..10 {
            let tmp = pool.clone();
            std::thread::spawn(move || {
                for i in 0..1_000_000 {
                    let mut reusable = tmp.pull().unwrap();
                    if i % 2 == 0 {
                        let mut vec = reusable.detach(Vec::new());
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

        assert_eq!(pool.count(), 10_000_000)
    }
}