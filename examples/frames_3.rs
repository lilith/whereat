//! Benchmark 3-frame deep traces
//! Run: cargo run --release --example frames_3

use errat::{At, ResultAtExt, at};
use std::hint::black_box;
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
struct E(u64, u64);
impl std::fmt::Display for E {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} {}", self.0, self.1)
    }
}

#[inline(never)]
fn level_3(i: u32, fail: u32) -> Result<u32, At<E>> {
    if i == fail {
        Err(at(E(i as u64, fail as u64)))
    } else {
        Ok(i * 2)
    }
}

#[inline(never)]
fn level_2(i: u32, fail: u32) -> Result<u32, At<E>> {
    level_3(i, fail).at()
}

#[inline(never)]
fn level_1(i: u32, fail: u32) -> Result<u32, At<E>> {
    level_2(i, fail).at()
}

fn main() {
    const ITERS: u32 = 10_000;

    // Warmup
    for i in 0..1000 {
        let _ = level_1(i, i);
    }

    let start = Instant::now();
    for i in 0..ITERS {
        let _ = black_box(level_1(black_box(i), black_box(i)));
    }
    let elapsed = start.elapsed();
    println!(
        "errat 3 frames, {} errors: {:.3}ms ({:.1}ns/error)",
        ITERS,
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_nanos() as f64 / ITERS as f64
    );

    let err = level_1(0, 0).unwrap_err();
    let frame_count = format!("{:?}", err).matches(" at ").count();
    println!("Frames in output: {}", frame_count);
}
