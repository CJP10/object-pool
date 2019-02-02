# Object Pool
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](
https://github.com/CJP10/object-pool)
[![Cargo](https://img.shields.io/crates/v/object-pool.svg)](
https://crates.io/crates/object-pool)
[![Documentation](https://docs.rs/object-pool/badge.svg)](
https://docs.rs/object-pool)
[![Rust 1.34+](https://img.shields.io/badge/rust-1.34+-lightgray.svg)](
https://www.rust-lang.org)

## This is nighly only as of 1.32 stable

A thread-safe object pool with automatic return and attach/detach semantics.

The goal of an object pool is to reuse expensive to allocate objects or frequently allocated objects
Common use case is when using buffer to read IO.

You would create a pool of size n, containing `Vec<u8>` that can be used to call something like `file.read_to_end(buff)`.
## Usage
```toml
[dependencies]
object-pool = "0.1"
```
```rust
extern crate object_pool;
```
Basic usage
```rust
let pool: Pool<Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096));
let mut reusable_buff = pool.pull().unwrap();
reusable_buff.clear();
some_file.read_to_end(reusable_buff);
//reusable_buff falls out of scope and is returned to the pool
```
For access across multiple threads simply wrap the pool in an [`Arc`]
```rust
let pool: Arc<Pool<T>> = Pool::new(cap, || T::new());
```

Check out the [docs] for more examples

## Performance
The benchmarks compare an `alloc()` vs a `pool.pull()` vs a `pool.detach()`.

Check out the [results]

[docs]: https://docs.rs/object-pool
[benches]: https://github.com/CJP10/object-pool/blob/master/src/lib.rs#L232
[`Arc`]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html
[results]: https://cjp10.github.io/object-pool/benches/criterion/report/index.html