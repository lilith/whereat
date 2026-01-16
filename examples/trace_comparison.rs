//! Print what each error handling approach actually captures
//!
//! Run: cargo run --example trace_comparison
//! With backtrace: RUST_BACKTRACE=1 cargo run --example trace_comparison

use std::panic::catch_unwind;

// ============================================================================
// 1. backtrace crate - full stack capture
// ============================================================================

#[inline(never)]
fn backtrace_level_3() -> backtrace::Backtrace {
    backtrace::Backtrace::new()
}

#[inline(never)]
fn backtrace_level_2() -> backtrace::Backtrace {
    backtrace_level_3()
}

#[inline(never)]
fn backtrace_level_1() -> backtrace::Backtrace {
    backtrace_level_2()
}

// ============================================================================
// 2. anyhow - optional backtrace via RUST_BACKTRACE
// ============================================================================

#[inline(never)]
fn anyhow_level_3() -> anyhow::Result<()> {
    anyhow::bail!("error at level 3")
}

#[inline(never)]
fn anyhow_level_2() -> anyhow::Result<()> {
    anyhow_level_3()
}

#[inline(never)]
fn anyhow_level_1() -> anyhow::Result<()> {
    anyhow_level_2()
}

// ============================================================================
// 3. panic + catch_unwind
// ============================================================================

#[inline(never)]
fn panic_level_3() {
    panic!("panic at level 3");
}

#[inline(never)]
fn panic_level_2() {
    panic_level_3();
}

#[inline(never)]
fn panic_level_1() {
    panic_level_2();
}

// ============================================================================
// 4. errat - #[track_caller] locations
// ============================================================================

use errat::{at, At, ResultAtExt};

#[derive(Debug)]
struct MyError(&'static str);

impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// NOTE: Do NOT put #[track_caller] on these functions!
// The .at() method already has #[track_caller], so it captures the correct location.
// If you add #[track_caller] to the outer function, ALL locations inside it
// will be reported as the caller's location instead.

#[inline(never)]
fn errat_level_3() -> Result<(), At<MyError>> {
    Err(at(MyError("error at level 3")))  // Line captured here
}

#[inline(never)]
fn errat_level_2() -> Result<(), At<MyError>> {
    errat_level_3().at()  // Line captured here
}

#[inline(never)]
fn errat_level_1() -> Result<(), At<MyError>> {
    errat_level_2().at()  // Line captured here
}

fn main() {
    println!("=== BACKTRACE CRATE ===");
    println!("Captures: Full native stack trace with all frames");
    println!();
    let bt = backtrace_level_1();
    // Print first 15 frames
    let formatted = format!("{:?}", bt);
    for (i, line) in formatted.lines().take(20).enumerate() {
        println!("  {}: {}", i, line);
    }
    let total_frames = formatted.lines().count();
    println!("  ... ({} total frames)", total_frames);

    println!();
    println!("=== ANYHOW (RUST_BACKTRACE={}) ===",
             std::env::var("RUST_BACKTRACE").unwrap_or_else(|_| "unset".into()));
    println!("Captures: Backtrace only if RUST_BACKTRACE=1");
    println!();
    let err = anyhow_level_1().unwrap_err();
    let debug = format!("{:?}", err);
    let lines: Vec<_> = debug.lines().collect();
    if lines.len() > 1 {
        for line in lines.iter().take(25) {
            println!("  {}", line);
        }
        if lines.len() > 25 {
            println!("  ... ({} total lines)", lines.len());
        }
    } else {
        println!("  Error: {}", err);
        println!("  (No backtrace - set RUST_BACKTRACE=1 to enable)");
    }

    println!();
    println!("=== PANIC + CATCH_UNWIND ===");
    println!("Captures: Panic message + location, backtrace via RUST_BACKTRACE");
    println!();
    let result = catch_unwind(|| panic_level_1());
    match result {
        Err(payload) => {
            if let Some(s) = payload.downcast_ref::<&str>() {
                println!("  Payload: {}", s);
            } else if let Some(s) = payload.downcast_ref::<String>() {
                println!("  Payload: {}", s);
            } else {
                println!("  Payload: (unknown type)");
            }
            println!("  (Backtrace printed to stderr if RUST_BACKTRACE=1)");
        }
        Ok(_) => println!("  No panic"),
    }

    println!();
    println!("=== ERRAT (#[track_caller]) ===");
    println!("Captures: Source location at each .at() call site");
    println!();
    let err = errat_level_1().unwrap_err();
    println!("  Display: {}", err);
    println!();
    println!("  Debug: {:?}", err);

    println!();
    println!("=== SUMMARY ===");
    println!("| Method     | What it captures                    | Cost      |");
    println!("|------------|-------------------------------------|-----------|");
    println!("| backtrace  | Full native stack (all frames)      | ~6µs      |");
    println!("| anyhow     | Optional backtrace (RUST_BACKTRACE) | 30ns/2µs  |");
    println!("| panic      | Message + optional backtrace        | ~1.3µs    |");
    println!("| errat      | Source locations via #[track_caller]| ~20-30ns  |");
}
