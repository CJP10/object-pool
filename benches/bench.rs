#[macro_use]
extern crate criterion;

use criterion::Criterion;
use object_pool::Pool;

static KB: usize = 1024;
static MB: usize = 1024 * KB;
static GB: usize = 1024 * MB;
static SIZES: &[usize] = &[
    4 * KB,
    16 * KB,
    64 * KB,
    128 * KB,
    512 * KB,
    1 * MB,
    16 * MB,
    32 * MB,
    64 * MB,
    128 * MB,
    256 * MB,
    512 * MB,
    1 * GB,
    2 * GB,
    3 * GB,
];

fn basics(c: &mut Criterion) {
    c.bench_function_over_inputs(
        "pulling_from_pool",
        |b, &&size| {
            let pool: Pool<Vec<u8>> = Pool::new(1, || Vec::with_capacity(size));
            b.iter(|| pool.try_pull())
        },
        SIZES,
    );

    c.bench_function_over_inputs(
        "detach_from_pool",
        |b, &&size| {
            let pool: Pool<Vec<u8>> = Pool::new(1, || Vec::with_capacity(size));
            b.iter(|| {
                let item = pool.try_pull().unwrap();
                let (_, vec) = item.detach();
                pool.attach(vec);
            })
        },
        SIZES,
    );

    c.bench_function_over_inputs(
        "alloc",
        |b, &&size| b.iter(|| Vec::<u8>::with_capacity(size)),
        SIZES,
    );
}

criterion_group!(benches, basics);
criterion_main!(benches);
