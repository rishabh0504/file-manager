use criterion::{black_box, criterion_group, criterion_main, Criterion};
use smart_file_manager::hasher::compute_chunk_size;

fn chunk_size_bench(c: &mut Criterion) {
    c.bench_function("compute_chunk_size_1mb", |b| {
        b.iter(|| black_box(compute_chunk_size(black_box(1024 * 1024))))
    });
}

criterion_group!(benches, chunk_size_bench);
criterion_main!(benches);
