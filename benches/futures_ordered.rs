use futures::executor::block_on;
use futures::stream::{FuturesOrdered, StreamExt};
use futures::Stream;
use futures::{executor::ThreadPool, task::SpawnExt};
use select_all_ordered::select_all_ordered_stream;

async fn workload(id: usize) -> usize {
    fibonacci(black_box(21)); //Some expensive calculation
    id
}

fn process_stream(mut stream: impl Stream<Item = usize> + std::marker::Unpin) {
    let mut next_id = 0;
    block_on(async move {
        while let Some(processed_frame) = stream.next().await {
            assert!(processed_frame == next_id);
            next_id += 1;
        }
    });
}

fn thiscrate(
    thread_pool: &ThreadPool,
    thread_pool_size: usize,
    frames: impl Iterator<Item = usize>,
) {
    let handles = frames.map(|frame| thread_pool.spawn_with_handle(workload(frame)).unwrap());
    let stream = select_all_ordered_stream(handles, thread_pool_size);
    let pinned_stream = Box::pin(stream);
    process_stream(pinned_stream);
}

fn futuresordered(
    thread_pool: &ThreadPool,
    _thread_pool_size: usize,
    frames: impl Iterator<Item = usize>,
) {
    let mut stream = FuturesOrdered::new();
    for frame in frames {
        stream.push_back(thread_pool.spawn_with_handle(workload(frame)).unwrap());
    }
    process_stream(stream);
}

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

pub fn criterion_benchmark(c: &mut Criterion) {
    let thread_pool_size = 4;
    let thread_pool = ThreadPool::builder()
        .pool_size(thread_pool_size)
        .create()
        .unwrap();

    let mut group = c.benchmark_group("Processing futures in parallel with ordered output");
    for i in [20, 40].iter() {
        let frames = (0..*i).map(|id| id).collect::<Vec<_>>();

        group.bench_with_input(BenchmarkId::new("This crate", i), i, |b, _i| {
            b.iter(|| thiscrate(&thread_pool, thread_pool_size, frames.clone().into_iter()))
        });
        group.bench_with_input(BenchmarkId::new("Futures ordered", i), i, |b, _i| {
            b.iter(|| futuresordered(&thread_pool, thread_pool_size, frames.clone().into_iter()))
        });
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
