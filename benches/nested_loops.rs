//! Benchmarks for nested loop tracing strategies.
//!
//! Compares different approaches to error tracing in nested loops:
//! 1. Plain enum - no tracing overhead
//! 2. thiserror - derive-based error with Display
//! 3. anyhow - dynamic error with context
//! 4. errat inner tracing - trace at every level (eager)
//! 5. errat outer tracing - delay tracing until outer loop (lazy)
//! 6. errat outer context - add context strings in outer loop
//! 7. backtrace - full stack capture via backtrace crate
//! 8. panic+catch_unwind - panic-based error handling
//!
//! Run with: cargo bench --bench nested_loops
//! Compare tinyvec: cargo bench --bench nested_loops --features tinyvec-64-bytes

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use errat::{at, At, ResultAtExt, ResultStartAtExt};
use std::panic::{catch_unwind, AssertUnwindSafe};

use core::fmt;

// ============================================================================
// Error types for comparison
// ============================================================================

/// Plain enum error (baseline - no derive, manual Display)
#[derive(Debug, Clone, Copy)]
enum LoopError {
    InnerFailed(u32),
    OuterFailed(u32),
}

impl fmt::Display for LoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoopError::InnerFailed(i) => write!(f, "inner loop failed at {}", i),
            LoopError::OuterFailed(i) => write!(f, "outer loop failed at {}", i),
        }
    }
}

/// thiserror-derived error
#[derive(Debug, thiserror::Error)]
enum ThisLoopError {
    #[error("inner loop failed at {0}")]
    InnerFailed(u32),
    #[error("outer loop failed at {0}")]
    OuterFailed(u32),
}

// ============================================================================
// Strategy 1: Inner tracing (eager) - trace at every call site
// ============================================================================

/// Innermost function - creates the error with trace
#[inline(never)]
fn inner_traced(i: u32, fail_at: u32) -> Result<u32, At<LoopError>> {
    if i == fail_at {
        Err(at(LoopError::InnerFailed(i)))
    } else {
        Ok(i * 2)
    }
}

/// Middle function - propagates with .at()
#[inline(never)]
fn middle_traced(i: u32, fail_at: u32) -> Result<u32, At<LoopError>> {
    inner_traced(i, fail_at).at()
}

/// Outer function - propagates with .at()
#[inline(never)]
fn outer_traced(i: u32, fail_at: u32) -> Result<u32, At<LoopError>> {
    middle_traced(i, fail_at).at()
}

// ============================================================================
// Strategy 2: Outer tracing (lazy) - plain Result until boundary
// ============================================================================

/// Innermost function - plain Result, no tracing
#[inline(never)]
fn inner_plain(i: u32, fail_at: u32) -> Result<u32, LoopError> {
    if i == fail_at {
        Err(LoopError::InnerFailed(i))
    } else {
        Ok(i * 2)
    }
}

/// Middle function - plain Result passthrough
#[inline(never)]
fn middle_plain(i: u32, fail_at: u32) -> Result<u32, LoopError> {
    inner_plain(i, fail_at)
}

/// Outer function - converts to At<E> at boundary
#[inline(never)]
fn outer_late_traced(i: u32, fail_at: u32) -> Result<u32, At<LoopError>> {
    middle_plain(i, fail_at).start_at()
}

// ============================================================================
// Strategy 3: Outer tracing with context
// ============================================================================

/// Outer function - adds context string at boundary
#[inline(never)]
fn outer_with_context(i: u32, fail_at: u32) -> Result<u32, At<LoopError>> {
    middle_plain(i, fail_at)
        .start_at()
        .at_str("processing batch")
}

/// Outer function - adds dynamic context at boundary
#[inline(never)]
fn outer_with_dynamic_context(i: u32, fail_at: u32) -> Result<u32, At<LoopError>> {
    middle_plain(i, fail_at)
        .start_at()
        .at_string(|| format!("processing item {}", i))
}

// ============================================================================
// Strategy 4: Baseline - no tracing at all
// ============================================================================

#[inline(never)]
fn outer_no_trace(i: u32, fail_at: u32) -> Result<u32, LoopError> {
    middle_plain(i, fail_at)
}

// ============================================================================
// Strategy 5: thiserror (derive-based, no backtrace)
// ============================================================================

#[inline(never)]
fn inner_thiserror(i: u32, fail_at: u32) -> Result<u32, ThisLoopError> {
    if i == fail_at {
        Err(ThisLoopError::InnerFailed(i))
    } else {
        Ok(i * 2)
    }
}

#[inline(never)]
fn middle_thiserror(i: u32, fail_at: u32) -> Result<u32, ThisLoopError> {
    inner_thiserror(i, fail_at)
}

#[inline(never)]
fn outer_thiserror(i: u32, fail_at: u32) -> Result<u32, ThisLoopError> {
    middle_thiserror(i, fail_at)
}

// ============================================================================
// Strategy 6: anyhow (dynamic error, with context)
// ============================================================================

#[inline(never)]
fn inner_anyhow(i: u32, fail_at: u32) -> anyhow::Result<u32> {
    if i == fail_at {
        anyhow::bail!("inner loop failed at {}", i)
    } else {
        Ok(i * 2)
    }
}

#[inline(never)]
fn middle_anyhow(i: u32, fail_at: u32) -> anyhow::Result<u32> {
    inner_anyhow(i, fail_at)
}

#[inline(never)]
fn outer_anyhow(i: u32, fail_at: u32) -> anyhow::Result<u32> {
    middle_anyhow(i, fail_at)
}

#[inline(never)]
fn outer_anyhow_with_context(i: u32, fail_at: u32) -> anyhow::Result<u32> {
    use anyhow::Context;
    middle_anyhow(i, fail_at).context("processing batch")
}

#[inline(never)]
fn outer_anyhow_with_dynamic_context(i: u32, fail_at: u32) -> anyhow::Result<u32> {
    use anyhow::Context;
    middle_anyhow(i, fail_at).with_context(|| format!("processing item {}", i))
}

// ============================================================================
// Strategy 7: backtrace crate (full stack capture)
// ============================================================================

/// Error with full backtrace attached
#[allow(dead_code)]
struct BacktraceError {
    kind: LoopError,
    backtrace: backtrace::Backtrace,
}

impl BacktraceError {
    #[inline(never)]
    fn new(kind: LoopError) -> Self {
        Self {
            kind,
            backtrace: backtrace::Backtrace::new(),
        }
    }
}

#[inline(never)]
fn inner_backtrace(i: u32, fail_at: u32) -> Result<u32, BacktraceError> {
    if i == fail_at {
        Err(BacktraceError::new(LoopError::InnerFailed(i)))
    } else {
        Ok(i * 2)
    }
}

#[inline(never)]
fn middle_backtrace(i: u32, fail_at: u32) -> Result<u32, BacktraceError> {
    inner_backtrace(i, fail_at)
}

#[inline(never)]
fn outer_backtrace(i: u32, fail_at: u32) -> Result<u32, BacktraceError> {
    middle_backtrace(i, fail_at)
}

/// Backtrace captured at boundary (late capture)
#[inline(never)]
fn outer_backtrace_late(i: u32, fail_at: u32) -> Result<u32, BacktraceError> {
    match middle_plain(i, fail_at) {
        Ok(v) => Ok(v),
        Err(kind) => Err(BacktraceError::new(kind)),
    }
}

// ============================================================================
// Strategy 8: panic + catch_unwind
// ============================================================================

/// Simulates panic-based error handling with catch_unwind
#[inline(never)]
fn outer_panic(i: u32, fail_at: u32) -> Result<u32, Box<dyn std::any::Any + Send>> {
    catch_unwind(AssertUnwindSafe(|| {
        if i == fail_at {
            panic!("inner loop failed at {}", i);
        }
        i * 2
    }))
}

/// Install silent panic hook for benchmark (suppress panic output)
fn install_silent_panic_hook() {
    std::panic::set_hook(Box::new(|_| {}));
}

// ============================================================================
// Nested loop simulation
// ============================================================================

/// Simulates nested loops with configurable error rate
fn run_nested_loops<E>(
    outer_count: u32,
    inner_count: u32,
    fail_every: u32, // 0 = no failures, N = fail every Nth iteration
    f: impl Fn(u32, u32) -> Result<u32, E>,
) -> (u64, u32) {
    let mut sum = 0u64;
    let mut errors = 0u32;

    for outer in 0..outer_count {
        for inner in 0..inner_count {
            let i = outer * inner_count + inner;
            let fail_at = if fail_every > 0 && i % fail_every == 0 {
                i
            } else {
                u32::MAX
            };

            // black_box the result to prevent loop elimination
            match black_box(f(black_box(i), black_box(fail_at))) {
                Ok(v) => sum += v as u64,
                Err(_) => errors += 1,
            }
        }
    }

    (sum, errors)
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_nested_no_errors(c: &mut Criterion) {
    let mut group = c.benchmark_group("nested_no_errors");

    // 100x100 = 10,000 iterations, 0% error rate
    const OUTER: u32 = 100;
    const INNER: u32 = 100;
    const FAIL_EVERY: u32 = 0; // No failures

    group.bench_function("plain_enum", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_no_trace))
    });

    group.bench_function("thiserror", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_thiserror))
    });

    group.bench_function("anyhow", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_anyhow))
    });

    group.bench_function("errat_inner", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_traced))
    });

    group.bench_function("errat_outer", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_late_traced))
    });

    group.bench_function("errat_outer_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_with_context))
    });

    group.finish();
}

fn bench_nested_1pct_errors(c: &mut Criterion) {
    let mut group = c.benchmark_group("nested_1pct_errors");

    // 100x100 = 10,000 iterations, 1% error rate
    const OUTER: u32 = 100;
    const INNER: u32 = 100;
    const FAIL_EVERY: u32 = 100; // 1% failure rate

    group.bench_function("plain_enum", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_no_trace))
    });

    group.bench_function("thiserror", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_thiserror))
    });

    group.bench_function("anyhow", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_anyhow))
    });

    group.bench_function("anyhow_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_anyhow_with_context))
    });

    group.bench_function("errat_inner", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_traced))
    });

    group.bench_function("errat_outer", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_late_traced))
    });

    group.bench_function("errat_outer_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_with_context))
    });

    group.bench_function("errat_outer_dyn_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_with_dynamic_context))
    });

    group.finish();
}

fn bench_nested_10pct_errors(c: &mut Criterion) {
    let mut group = c.benchmark_group("nested_10pct_errors");

    // 100x100 = 10,000 iterations, 10% error rate
    const OUTER: u32 = 100;
    const INNER: u32 = 100;
    const FAIL_EVERY: u32 = 10; // 10% failure rate

    group.bench_function("plain_enum", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_no_trace))
    });

    group.bench_function("thiserror", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_thiserror))
    });

    group.bench_function("anyhow", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_anyhow))
    });

    group.bench_function("anyhow_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_anyhow_with_context))
    });

    group.bench_function("errat_inner", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_traced))
    });

    group.bench_function("errat_outer", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_late_traced))
    });

    group.bench_function("errat_outer_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_with_context))
    });

    group.finish();
}

fn bench_nested_100pct_errors(c: &mut Criterion) {
    let mut group = c.benchmark_group("nested_100pct_errors");

    // 100x100 = 10,000 iterations, 100% error rate (worst case)
    const OUTER: u32 = 100;
    const INNER: u32 = 100;
    const FAIL_EVERY: u32 = 1; // 100% failure rate

    group.bench_function("plain_enum", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_no_trace))
    });

    group.bench_function("thiserror", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_thiserror))
    });

    group.bench_function("anyhow", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_anyhow))
    });

    group.bench_function("anyhow_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_anyhow_with_context))
    });

    group.bench_function("anyhow_dyn_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_anyhow_with_dynamic_context))
    });

    group.bench_function("errat_inner", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_traced))
    });

    group.bench_function("errat_outer", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_late_traced))
    });

    group.bench_function("errat_outer_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_with_context))
    });

    group.bench_function("errat_outer_dyn_ctx", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_with_dynamic_context))
    });

    // Full backtrace capture (SLOW - captures entire stack)
    group.bench_function("backtrace", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_backtrace))
    });

    group.bench_function("backtrace_late", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_backtrace_late))
    });

    // Panic + catch_unwind (VERY SLOW - unwinds stack on every error)
    install_silent_panic_hook();
    group.bench_function("panic_unwind", |b| {
        b.iter(|| run_nested_loops(OUTER, INNER, FAIL_EVERY, outer_panic))
    });

    group.finish();
}

fn bench_trace_strategy_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("trace_strategy");

    // Fixed: 10% error rate, vary inner loop count
    const OUTER: u32 = 100;
    const FAIL_EVERY: u32 = 10;

    for inner in [10, 50, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("inner_traced", inner),
            &inner,
            |b, &inner| b.iter(|| run_nested_loops(OUTER, inner, FAIL_EVERY, outer_traced)),
        );

        group.bench_with_input(
            BenchmarkId::new("outer_late_traced", inner),
            &inner,
            |b, &inner| b.iter(|| run_nested_loops(OUTER, inner, FAIL_EVERY, outer_late_traced)),
        );

        group.bench_with_input(
            BenchmarkId::new("outer_with_context", inner),
            &inner,
            |b, &inner| b.iter(|| run_nested_loops(OUTER, inner, FAIL_EVERY, outer_with_context)),
        );
    }

    group.finish();
}

fn bench_single_error_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_error");

    // Measure overhead of a single error creation
    group.bench_function("plain_enum", |b| {
        b.iter(|| {
            let err: Result<u32, LoopError> = Err(black_box(LoopError::InnerFailed(42)));
            black_box(err)
        })
    });

    group.bench_function("thiserror", |b| {
        b.iter(|| {
            let err: Result<u32, ThisLoopError> = Err(black_box(ThisLoopError::InnerFailed(42)));
            black_box(err)
        })
    });

    group.bench_function("anyhow", |b| {
        b.iter(|| {
            let err: anyhow::Result<u32> = Err(anyhow::anyhow!("inner loop failed at {}", 42));
            black_box(err)
        })
    });

    group.bench_function("anyhow_ctx", |b| {
        b.iter(|| {
            use anyhow::Context;
            let err: anyhow::Result<u32> = Err(anyhow::anyhow!("inner loop failed"))
                .context("processing");
            black_box(err)
        })
    });

    group.bench_function("errat_1_frame", |b| {
        b.iter(|| {
            let err: Result<u32, At<LoopError>> = Err(at(black_box(LoopError::InnerFailed(42))));
            black_box(err)
        })
    });

    group.bench_function("errat_3_frames", |b| {
        b.iter(|| {
            let err = outer_traced(black_box(0), black_box(0));
            black_box(err)
        })
    });

    group.bench_function("errat_late_1_frame", |b| {
        b.iter(|| {
            let err = outer_late_traced(black_box(0), black_box(0));
            black_box(err)
        })
    });

    group.bench_function("errat_static_ctx", |b| {
        b.iter(|| {
            let err = outer_with_context(black_box(0), black_box(0));
            black_box(err)
        })
    });

    group.bench_function("errat_dynamic_ctx", |b| {
        b.iter(|| {
            let err = outer_with_dynamic_context(black_box(0), black_box(0));
            black_box(err)
        })
    });

    // Full backtrace capture
    group.bench_function("backtrace", |b| {
        b.iter(|| {
            let err = BacktraceError::new(black_box(LoopError::InnerFailed(42)));
            black_box(err)
        })
    });

    // Panic + catch_unwind
    install_silent_panic_hook();
    group.bench_function("panic_unwind", |b| {
        b.iter(|| {
            let result = outer_panic(black_box(0), black_box(0));
            black_box(result)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_nested_no_errors,
    bench_nested_1pct_errors,
    bench_nested_10pct_errors,
    bench_nested_100pct_errors,
    bench_trace_strategy_comparison,
    bench_single_error_overhead,
);

criterion_main!(benches);
