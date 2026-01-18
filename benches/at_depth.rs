//! Focused benchmark: cost of 1, 4, 8, 16 at() calls on error path.
//!
//! Run with: cargo bench --bench at_depth

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main, measurement::WallTime};
use std::hint::black_box;
use whereat::{At, ResultAtExt, at};

use core::fmt;

#[derive(Debug, Clone)]
struct TestError;

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error")
    }
}

#[inline(never)]
fn chain_at(depth: u32) -> Result<(), At<TestError>> {
    if depth == 0 {
        Err(at(TestError))
    } else {
        chain_at(depth - 1).at()
    }
}

fn bench_at_depth(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("at_depth");
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.measurement_time(std::time::Duration::from_secs(1));
    group.sample_size(30);

    for depth in [1, 4, 8, 16] {
        group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, &depth| {
            b.iter(|| {
                let _ = chain_at(black_box(depth));
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_at_depth);
criterion_main!(benches);
