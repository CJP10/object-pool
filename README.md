# Object Pool
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](
https://github.com/CJP10/object-pool)
[![Cargo](https://img.shields.io/crates/v/object-pool.svg)](
https://crates.io/crates/object-pool)
[![Documentation](https://docs.rs/object-pool/badge.svg)](
https://docs.rs/object-pool)

A thread-safe object pool with automatic return and attach/detach semantics

The goal of an object pool is to reuse expensive to allocate objects or frequently allocated objects

## Usage
```toml
[dependencies]
object-pool = "0.5"
```
```rust
extern crate object_pool;
```
## Examples

### Creating a Pool

The general pool creation looks like this
```rust
 let pool: Pool<T> = Pool::new(capacity, || T::new());
```
Example pool with 32 `Vec<u8>` with capacity of 4096
```rust
 let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096));
```

### Using a Pool

Basic usage for pulling from the pool
```rust
let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096));
let mut reusable_buff = pool.pull().unwrap(); // returns None when the pool is saturated
reusable_buff.clear(); // clear the buff before using
some_file.read_to_end(reusable_buff);
// reusable_buff is automatically returned to the pool when it goes out of scope
```
Pull from pool and `detach()`
```rust
let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096));
let mut reusable_buff = pool.pull().unwrap(); // returns None when the pool is saturated
reusable_buff.clear(); // clear the buff before using
let (pool, reusable_buff) = reusable_buff.detach();
let mut s = String::from(reusable_buff);
s.push_str("hello, world!");
pool.attach(s.into_bytes()); // reattach the buffer before reusable goes out of scope
// reusable_buff is automatically returned to the pool when it goes out of scope
```

### Using Across Threads

You simply wrap the pool in a [`std::sync::Arc`]
```rust
let pool: Arc<Pool<T>> = Arc::new(Pool::new(cap, || T::new()));
```

## Warning

Objects in the pool are not automatically reset, they are returned but NOT reset
You may want to call `object.reset()` or  `object.clear()`
or any other equivalent for the object that you are using, after pulling from the pool

Check out the [docs] for more examples

## Performance
The benchmarks compare `alloc()` vs `pool.pull()` vs `pool.detach()`.

Check out the [results]

For those who don't like graphs, here's the [raw output]

[raw output]: https://github.com/CJP10/object-pool/blob/master/BENCHMARK.md
[docs]: https://docs.rs/object-pool
[benches]: https://github.com/CJP10/object-pool/blob/master/src/lib.rs#L232
[`Arc`]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html
[results]: https://cjp10.github.io/object-pool/benches/criterion/report/index.html
