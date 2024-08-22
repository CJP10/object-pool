#[macro_use]
extern crate criterion;

use criterion::Criterion;
use object_pool::{experimental::Pool as ExperimentalPool, Pool};
use std::iter::FromIterator;
use std::sync::Arc;

fn basics(c: &mut Criterion) {
    let mut group = c.benchmark_group("pulling_from_pool");
    group.throughput(criterion::Throughput::Elements(1));

    group.bench_function("experimental_borrowed", |b| {
        let pool = ExperimentalPool::from_iter(&[()]);
        b.iter(|| pool.pull())
    });

    group.bench_function("experimental_owned", |b| {
        let pool = Arc::new(ExperimentalPool::from_iter(&[()]));
        b.iter(|| pool.pull_owned())
    });

    group.bench_function("borrowed", |b| {
        let pool = Pool::new(1, || ());
        b.iter(|| pool.try_pull())
    });

    group.bench_function("owned", |b| {
        let pool = std::sync::Arc::new(Pool::new(1, || ()));
        b.iter(|| pool.try_pull_owned())
    });
    drop(group);

    c.bench_function("detach_from_pool", |b| {
        let pool = Pool::new(1, || ());
        b.iter(|| {
            let item = pool.try_pull().unwrap();
            let (_, vec) = item.detach();
            pool.attach(vec);
        })
    });
}

criterion_group!(benches, basics);
criterion_main!(benches);
