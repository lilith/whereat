//! Benchmark 25-frame deep traces (fits in 28 slots without spill)
//! Run: cargo run --release --example frames_25

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

macro_rules! make_levels {
    ($($name:ident => $next:ident),+ $(,)?) => {
        $(
            #[inline(never)]
            fn $name(i: u32, fail: u32) -> Result<u32, At<E>> {
                $next(i, fail).at()
            }
        )+
    };
}

#[inline(never)]
fn level_25(i: u32, fail: u32) -> Result<u32, At<E>> {
    if i == fail {
        Err(at(E(i as u64, fail as u64)))
    } else {
        Ok(i * 2)
    }
}

make_levels!(
    level_24 => level_25, level_23 => level_24, level_22 => level_23, level_21 => level_22,
    level_20 => level_21, level_19 => level_20, level_18 => level_19, level_17 => level_18,
    level_16 => level_17, level_15 => level_16, level_14 => level_15, level_13 => level_14,
    level_12 => level_13, level_11 => level_12, level_10 => level_11, level_9 => level_10,
    level_8 => level_9, level_7 => level_8, level_6 => level_7, level_5 => level_6,
    level_4 => level_5, level_3 => level_4, level_2 => level_3, level_1 => level_2,
);

fn main() {
    const ITERS: u32 = 10_000;

    // Warmup
    for i in 0..1000 {
        let _ = level_1(i, i);
    }

    // 25 frames, 100% error rate
    let start = Instant::now();
    for i in 0..ITERS {
        let _ = black_box(level_1(black_box(i), black_box(i)));
    }
    let elapsed = start.elapsed();
    println!(
        "errat 25 frames, {} errors: {:.3}ms ({:.1}ns/error)",
        ITERS,
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_nanos() as f64 / ITERS as f64
    );

    // Verify frame count
    let err = level_1(0, 0).unwrap_err();
    let frame_count = format!("{:?}", err).matches(" at ").count();
    println!("Frames in output: {}", frame_count);
}
