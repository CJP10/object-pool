# Object Pool
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](
https://github.com/CJP10/object-pool)
[![Cargo](https://img.shields.io/crates/v/object-pool.svg)](
https://crates.io/crates/object-pool)
[![Documentation](https://docs.rs/object-pool/badge.svg)](
https://docs.rs/object-pool)

A thread-safe object pool with automatic return and attach/detach semantics.

The goal of an object pool is to reuse expensive to allocate objects or frequently allocated objects
Common use case is when using a buffer to read IO.

## Usage
```toml
[dependencies]
object-pool = "0.4"
```
```rust
extern crate object_pool;
```
### Basic usage
```rust
let pool: Pool<'_, Vec<u8>> = Pool::new(32, || Vec::with_capacity(4096));
let mut reusable_buff = pool.pull().unwrap();
reusable_buff.clear();
some_file.read_to_end(reusable_buff);
// reusable_buff falls out of scope and is returned to the pool
```
For access across multiple threads simply wrap the pool in an [`Arc`]
```rust
let pool: Arc<Pool<'a, Vec<u8>>> = Pool::new(32, || Vec::with_capacity(4096));
```

Sending pooled resources across threads is possible, but requires the pool's lifetime be static
```rust
lazy_static! {
    static ref POOL: Arc<Pool<'static, Vec<u8>>> = Arc::new(Pool::new(32, || Vec::with_capacity(4096)));
}
```

Check out the [docs] for more examples

## Performance
The benchmarks compare `alloc()` vs `pool.pull()` vs `pool.detach()` vs `lifeguard` vs `WIP SyncPool`.

Check out the [results]

For those who don't like graphs, here's the [raw output]

[raw output]: https://github.com/CJP10/object-pool/blob/master/BENCHMARK.md
[docs]: https://docs.rs/object-pool
[benches]: https://github.com/CJP10/object-pool/blob/master/src/lib.rs#L232
[`Arc`]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html
[results]: https://cjp10.github.io/object-pool/benches/criterion/report/index.html
