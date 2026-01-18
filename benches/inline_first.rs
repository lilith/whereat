//! Benchmark prototype for inline-first-frame optimization.
//!
//! Compares current AtTraceBoxed (always heap) vs InlineFirstTrace (inline first frame).

// Allow Box<Vec<..>> - intentional to test inline storage with minimal struct size
#![allow(clippy::box_collection)]

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::panic::Location;

type Loc = &'static Location<'static>;

// ============================================================================
// Current implementation (simplified)
// ============================================================================

struct CurrentTrace {
    inner: Option<Box<CurrentTraceInner>>,
}

struct CurrentTraceInner {
    locations: Vec<&'static Location<'static>>,
}

impl CurrentTrace {
    #[inline]
    const fn new() -> Self {
        Self { inner: None }
    }

    #[inline]
    #[track_caller]
    fn push(&mut self) {
        let loc = Location::caller();
        let inner = self.inner.get_or_insert_with(|| {
            Box::new(CurrentTraceInner {
                locations: Vec::with_capacity(12),
            })
        });
        inner.locations.push(loc);
    }

    #[inline]
    fn frame_count(&self) -> usize {
        self.inner.as_ref().map_or(0, |i| i.locations.len())
    }
}

// ============================================================================
// New implementation: Inline first frame
// ============================================================================

struct InlineFirstTrace {
    // First location stored inline - no allocation for 1-frame traces
    first: Option<&'static Location<'static>>,
    // Rest allocated lazily only when needed
    rest: Option<Box<InlineFirstRest>>,
}

struct InlineFirstRest {
    locations: Vec<&'static Location<'static>>,
    // Could also store crate_info, contexts here
}

impl InlineFirstTrace {
    #[inline]
    const fn new() -> Self {
        Self {
            first: None,
            rest: None,
        }
    }

    #[inline]
    #[track_caller]
    fn push(&mut self) {
        let loc = Location::caller();
        match self.first {
            None => {
                // First frame - store inline, zero allocation
                self.first = Some(loc);
            }
            Some(_) => {
                // Subsequent frames - allocate rest
                let rest = self.rest.get_or_insert_with(|| {
                    Box::new(InlineFirstRest {
                        locations: Vec::with_capacity(11), // 12 - 1 for first
                    })
                });
                rest.locations.push(loc);
            }
        }
    }

    #[inline]
    fn frame_count(&self) -> usize {
        let first = if self.first.is_some() { 1 } else { 0 };
        let rest = self.rest.as_ref().map_or(0, |r| r.locations.len());
        first + rest
    }
}

// ============================================================================
// Inline 2 locations (16 bytes inline storage)
// ============================================================================

struct Inline2Trace {
    // Two locations inline - covers 90%+ of real traces
    inline: [Option<Loc>; 2],    // 16 bytes
    rest: Option<Box<Vec<Loc>>>, // 8 bytes, rarely used
}

impl Inline2Trace {
    #[inline]
    const fn new() -> Self {
        Self {
            inline: [None, None],
            rest: None,
        }
    }

    #[inline]
    #[track_caller]
    fn push(&mut self) {
        let loc = Location::caller();
        if self.inline[0].is_none() {
            self.inline[0] = Some(loc);
        } else if self.inline[1].is_none() {
            self.inline[1] = Some(loc);
        } else {
            self.rest
                .get_or_insert_with(|| Box::new(Vec::with_capacity(10)))
                .push(loc);
        }
    }

    #[inline]
    fn frame_count(&self) -> usize {
        let inline = self.inline.iter().filter(|x| x.is_some()).count();
        let rest = self.rest.as_ref().map_or(0, |v| v.len());
        inline + rest
    }
}

// ============================================================================
// Inline 3 locations (24 bytes inline storage)
// ============================================================================

struct Inline3Trace {
    inline: [Option<Loc>; 3],    // 24 bytes
    rest: Option<Box<Vec<Loc>>>, // 8 bytes
}

impl Inline3Trace {
    #[inline]
    const fn new() -> Self {
        Self {
            inline: [None, None, None],
            rest: None,
        }
    }

    #[inline]
    #[track_caller]
    fn push(&mut self) {
        let loc = Location::caller();
        for slot in &mut self.inline {
            if slot.is_none() {
                *slot = Some(loc);
                return;
            }
        }
        self.rest
            .get_or_insert_with(|| Box::new(Vec::with_capacity(9)))
            .push(loc);
    }

    #[inline]
    fn frame_count(&self) -> usize {
        let inline = self.inline.iter().filter(|x| x.is_some()).count();
        let rest = self.rest.as_ref().map_or(0, |v| v.len());
        inline + rest
    }
}

// ============================================================================
// Inline 3 with count (avoid iteration for frame_count)
// ============================================================================

struct Inline3CountTrace {
    inline: [Option<Loc>; 3],    // 24 bytes
    count: u8,                   // 1 byte (padded to 8)
    rest: Option<Box<Vec<Loc>>>, // 8 bytes
}

impl Inline3CountTrace {
    #[inline]
    const fn new() -> Self {
        Self {
            inline: [None, None, None],
            count: 0,
            rest: None,
        }
    }

    #[inline]
    #[track_caller]
    fn push(&mut self) {
        let loc = Location::caller();
        let idx = self.count as usize;
        if idx < 3 {
            self.inline[idx] = Some(loc);
            self.count += 1;
        } else {
            self.rest
                .get_or_insert_with(|| Box::new(Vec::with_capacity(9)))
                .push(loc);
        }
    }

    #[inline]
    fn frame_count(&self) -> usize {
        let rest = self.rest.as_ref().map_or(0, |v| v.len());
        self.count as usize + rest
    }
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_single_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_frame");

    group.bench_function("current_1fr", |b| {
        b.iter(|| {
            let mut trace = CurrentTrace::new();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline_first_1fr", |b| {
        b.iter(|| {
            let mut trace = InlineFirstTrace::new();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline2_1fr", |b| {
        b.iter(|| {
            let mut trace = Inline2Trace::new();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3_1fr", |b| {
        b.iter(|| {
            let mut trace = Inline3Trace::new();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3count_1fr", |b| {
        b.iter(|| {
            let mut trace = Inline3CountTrace::new();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    group.finish();
}

fn bench_multi_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_frame");

    // 2 frames
    group.bench_function("current_2fr", |b| {
        b.iter(|| {
            let mut trace = CurrentTrace::new();
            trace.push();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline2_2fr", |b| {
        b.iter(|| {
            let mut trace = Inline2Trace::new();
            trace.push();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3count_2fr", |b| {
        b.iter(|| {
            let mut trace = Inline3CountTrace::new();
            trace.push();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    // 3 frames
    group.bench_function("current_3fr", |b| {
        b.iter(|| {
            let mut trace = CurrentTrace::new();
            trace.push();
            trace.push();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3_3fr", |b| {
        b.iter(|| {
            let mut trace = Inline3Trace::new();
            trace.push();
            trace.push();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3count_3fr", |b| {
        b.iter(|| {
            let mut trace = Inline3CountTrace::new();
            trace.push();
            trace.push();
            trace.push();
            black_box(trace.frame_count())
        })
    });

    // 5 frames (tests spill to heap)
    group.bench_function("current_5fr", |b| {
        b.iter(|| {
            let mut trace = CurrentTrace::new();
            for _ in 0..5 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3count_5fr", |b| {
        b.iter(|| {
            let mut trace = Inline3CountTrace::new();
            for _ in 0..5 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    // 10 frames
    group.bench_function("current_10fr", |b| {
        b.iter(|| {
            let mut trace = CurrentTrace::new();
            for _ in 0..10 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3count_10fr", |b| {
        b.iter(|| {
            let mut trace = Inline3CountTrace::new();
            for _ in 0..10 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    // 15 frames - compare all inline variants
    group.bench_function("current_15fr", |b| {
        b.iter(|| {
            let mut trace = CurrentTrace::new();
            for _ in 0..15 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline_first_15fr", |b| {
        b.iter(|| {
            let mut trace = InlineFirstTrace::new();
            for _ in 0..15 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline2_15fr", |b| {
        b.iter(|| {
            let mut trace = Inline2Trace::new();
            for _ in 0..15 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3_15fr", |b| {
        b.iter(|| {
            let mut trace = Inline3Trace::new();
            for _ in 0..15 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3count_15fr", |b| {
        b.iter(|| {
            let mut trace = Inline3CountTrace::new();
            for _ in 0..15 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    // 20 frames
    group.bench_function("current_20fr", |b| {
        b.iter(|| {
            let mut trace = CurrentTrace::new();
            for _ in 0..20 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3count_20fr", |b| {
        b.iter(|| {
            let mut trace = Inline3CountTrace::new();
            for _ in 0..20 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    // 30 frames
    group.bench_function("current_30fr", |b| {
        b.iter(|| {
            let mut trace = CurrentTrace::new();
            for _ in 0..30 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    group.bench_function("inline3count_30fr", |b| {
        b.iter(|| {
            let mut trace = Inline3CountTrace::new();
            for _ in 0..30 {
                trace.push();
            }
            black_box(trace.frame_count())
        })
    });

    group.finish();
}

fn bench_realistic(c: &mut Criterion) {
    let mut group = c.benchmark_group("realistic");

    // Simulate real error creation: wrap error + capture location
    #[derive(Debug)]
    struct DummyError;

    group.bench_function("current_at_equivalent", |b| {
        b.iter(|| {
            let err = black_box(DummyError);
            let mut trace = CurrentTrace::new();
            trace.push();
            black_box((err, trace.frame_count()))
        })
    });

    group.bench_function("inline_first_at_equivalent", |b| {
        b.iter(|| {
            let err = black_box(DummyError);
            let mut trace = InlineFirstTrace::new();
            trace.push();
            black_box((err, trace.frame_count()))
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_single_frame,
    bench_multi_frame,
    bench_realistic
);
criterion_main!(benches);
