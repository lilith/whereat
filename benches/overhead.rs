//! Benchmarks for errat overhead in various scenarios.
//!
//! Run with: cargo bench
//! Run specific benchmark: cargo bench --bench overhead -- "hot_loop"

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use errat::{at, At, ResultAtExt, ResultStartAtExt};

use core::fmt;

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum BenchError {
    NotFound,
    InvalidInput,
}

impl fmt::Display for BenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BenchError::NotFound => write!(f, "not found"),
            BenchError::InvalidInput => write!(f, "invalid input"),
        }
    }
}

// ============================================================================
// Baseline: No error handling
// ============================================================================

fn baseline_no_error(n: u64) -> u64 {
    if n == 0 { 0 } else { n * 2 }
}

// ============================================================================
// Plain Result (no tracing)
// ============================================================================

fn plain_result_ok(n: u64) -> Result<u64, BenchError> {
    if n == 0 {
        Err(BenchError::NotFound)
    } else {
        Ok(n * 2)
    }
}

fn plain_result_err(_n: u64) -> Result<u64, BenchError> {
    Err(BenchError::NotFound)
}

// ============================================================================
// At<E> (errat tracing)
// ============================================================================

fn at_result_ok(n: u64) -> Result<u64, At<BenchError>> {
    if n == 0 {
        Err(at(BenchError::NotFound))
    } else {
        Ok(n * 2)
    }
}

fn at_result_err(_n: u64) -> Result<u64, At<BenchError>> {
    Err(at(BenchError::NotFound))
}

fn at_result_with_context(_n: u64) -> Result<u64, At<BenchError>> {
    Err(at(BenchError::NotFound).at_str("not found in cache"))
}

fn at_result_with_many_contexts(_n: u64) -> Result<u64, At<BenchError>> {
    Err(at(BenchError::NotFound)
        .at_str("context 1")
        .at_str("context 2")
        .at_str("context 3")
        .at_str("context 4")
        .at_str("context 5"))
}

// ============================================================================
// Call chain scenarios
// ============================================================================

fn chain_plain_3_levels(n: u64) -> Result<u64, BenchError> {
    fn level_2(n: u64) -> Result<u64, BenchError> {
        if n == 0 { Err(BenchError::NotFound) } else { Ok(n) }
    }
    fn level_1(n: u64) -> Result<u64, BenchError> {
        level_2(n)
    }
    level_1(n).map(|v| v * 2)
}

fn chain_at_3_levels(n: u64) -> Result<u64, At<BenchError>> {
    fn level_2(n: u64) -> Result<u64, At<BenchError>> {
        if n == 0 { Err(at(BenchError::NotFound)) } else { Ok(n) }
    }
    fn level_1(n: u64) -> Result<u64, At<BenchError>> {
        level_2(n).at()
    }
    level_1(n).at().map(|v| v * 2)
}

fn chain_at_10_levels(n: u64) -> Result<u64, At<BenchError>> {
    fn level(depth: u32, n: u64) -> Result<u64, At<BenchError>> {
        if depth == 0 {
            if n == 0 { Err(at(BenchError::NotFound)) } else { Ok(n) }
        } else {
            level(depth - 1, n).at()
        }
    }
    level(9, n).at()
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_happy_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("happy_path");

    // Input that succeeds (n=1)
    let n = 1u64;

    group.bench_function("baseline", |b| {
        b.iter(|| baseline_no_error(black_box(n)))
    });

    group.bench_function("plain_result", |b| {
        b.iter(|| plain_result_ok(black_box(n)))
    });

    group.bench_function("at_result", |b| {
        b.iter(|| at_result_ok(black_box(n)))
    });

    group.bench_function("chain_plain_3", |b| {
        b.iter(|| chain_plain_3_levels(black_box(n)))
    });

    group.bench_function("chain_at_3", |b| {
        b.iter(|| chain_at_3_levels(black_box(n)))
    });

    group.bench_function("chain_at_10", |b| {
        b.iter(|| chain_at_10_levels(black_box(n)))
    });

    group.finish();
}

fn bench_error_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("error_path");

    // Input that fails (n=0)
    let n = 0u64;

    group.bench_function("plain_result", |b| {
        b.iter(|| {
            let _ = plain_result_err(black_box(n));
        })
    });

    group.bench_function("at_result", |b| {
        b.iter(|| {
            let _ = at_result_err(black_box(n));
        })
    });

    group.bench_function("at_with_context", |b| {
        b.iter(|| {
            let _ = at_result_with_context(black_box(n));
        })
    });

    group.bench_function("at_with_5_contexts", |b| {
        b.iter(|| {
            let _ = at_result_with_many_contexts(black_box(n));
        })
    });

    group.bench_function("chain_plain_3", |b| {
        b.iter(|| {
            let _ = chain_plain_3_levels(black_box(n));
        })
    });

    group.bench_function("chain_at_3", |b| {
        b.iter(|| {
            let _ = chain_at_3_levels(black_box(n));
        })
    });

    group.bench_function("chain_at_10", |b| {
        b.iter(|| {
            let _ = chain_at_10_levels(black_box(n));
        })
    });

    group.finish();
}

fn bench_hot_loop(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot_loop");

    // Simulate processing 1000 items with 1% error rate
    const ITEMS: u64 = 1000;

    group.bench_function("plain_1pct_errors", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for i in 0..ITEMS {
                if let Ok(v) = plain_result_ok(black_box(i % 100)) {
                    sum += v;
                }
            }
            sum
        })
    });

    group.bench_function("at_1pct_errors", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for i in 0..ITEMS {
                if let Ok(v) = at_result_ok(black_box(i % 100)) {
                    sum += v;
                }
            }
            sum
        })
    });

    // 100% error rate - worst case
    group.bench_function("plain_100pct_errors", |b| {
        b.iter(|| {
            for i in 0..ITEMS {
                let _ = plain_result_err(black_box(i));
            }
        })
    });

    group.bench_function("at_100pct_errors", |b| {
        b.iter(|| {
            for i in 0..ITEMS {
                let _ = at_result_err(black_box(i));
            }
        })
    });

    group.bench_function("at_100pct_with_context", |b| {
        b.iter(|| {
            for i in 0..ITEMS {
                let _ = at_result_with_context(black_box(i));
            }
        })
    });

    group.finish();
}

fn bench_trace_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("trace_depth");

    for depth in [1, 5, 10, 20, 50, 100] {
        group.bench_with_input(BenchmarkId::new("at_chain", depth), &depth, |b, &depth| {
            b.iter(|| {
                fn recurse(d: u32) -> Result<(), At<BenchError>> {
                    if d == 0 {
                        Err(at(BenchError::NotFound))
                    } else {
                        recurse(d - 1).at()
                    }
                }
                let _ = recurse(black_box(depth));
            })
        });
    }

    group.finish();
}

fn bench_context_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_count");

    for count in [0, 1, 5, 10, 20] {
        group.bench_with_input(BenchmarkId::new("static_str", count), &count, |b, &count| {
            b.iter(|| {
                let mut err = at(BenchError::NotFound);
                for _ in 0..count {
                    err = err.at_str("context");
                }
                black_box(err)
            })
        });
    }

    for count in [0, 1, 5, 10, 20] {
        group.bench_with_input(BenchmarkId::new("dynamic_string", count), &count, |b, &count| {
            b.iter(|| {
                let mut err = at(BenchError::NotFound);
                for i in 0..count {
                    err = err.at_string(|| format!("context {}", i));
                }
                black_box(err)
            })
        });
    }

    group.finish();
}

fn bench_display_format(c: &mut Criterion) {
    let mut group = c.benchmark_group("display_format");

    // Simple error, no trace
    let simple = at(BenchError::NotFound);
    group.bench_function("simple_display", |b| {
        b.iter(|| format!("{}", black_box(&simple)))
    });
    group.bench_function("simple_debug", |b| {
        b.iter(|| format!("{:?}", black_box(&simple)))
    });

    // Error with 5-level trace
    fn make_deep() -> At<BenchError> {
        fn level(d: u32) -> Result<(), At<BenchError>> {
            if d == 0 { Err(at(BenchError::NotFound)) } else { level(d - 1).at() }
        }
        level(4).unwrap_err()
    }
    let deep = make_deep();
    group.bench_function("deep_display", |b| {
        b.iter(|| format!("{}", black_box(&deep)))
    });
    group.bench_function("deep_debug", |b| {
        b.iter(|| format!("{:?}", black_box(&deep)))
    });

    // Error with many contexts
    let contextual = at(BenchError::NotFound)
        .at_str("context 1")
        .at_str("context 2")
        .at_str("context 3")
        .at_str("context 4")
        .at_str("context 5");
    group.bench_function("contextual_display", |b| {
        b.iter(|| format!("{}", black_box(&contextual)))
    });
    group.bench_function("contextual_debug", |b| {
        b.iter(|| format!("{:?}", black_box(&contextual)))
    });

    group.finish();
}

fn bench_start_at(c: &mut Criterion) {
    let mut group = c.benchmark_group("start_at");

    fn external_error() -> Result<(), &'static str> {
        Err("external error")
    }

    group.bench_function("start_at_conversion", |b| {
        b.iter(|| {
            let _ = external_error().start_at();
        })
    });

    group.bench_function("start_at_late_conversion", |b| {
        b.iter(|| {
            let _ = external_error().start_at_late();
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_happy_path,
    bench_error_path,
    bench_hot_loop,
    bench_trace_depth,
    bench_context_count,
    bench_display_format,
    bench_start_at,
);

criterion_main!(benches);
