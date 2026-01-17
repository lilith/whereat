//! Count how many frames backtrace captures at various call depths
use backtrace::Backtrace;
use std::hint::black_box;

#[inline(never)]
fn level_10(x: u32) -> (u32, Backtrace) {
    (black_box(x + 1), Backtrace::new())
}

#[inline(never)]
fn level_9(x: u32) -> (u32, Backtrace) {
    let (y, bt) = level_10(x);
    (black_box(y + 1), bt)
}
#[inline(never)]
fn level_8(x: u32) -> (u32, Backtrace) {
    let (y, bt) = level_9(x);
    (black_box(y + 1), bt)
}
#[inline(never)]
fn level_7(x: u32) -> (u32, Backtrace) {
    let (y, bt) = level_8(x);
    (black_box(y + 1), bt)
}
#[inline(never)]
fn level_6(x: u32) -> (u32, Backtrace) {
    let (y, bt) = level_7(x);
    (black_box(y + 1), bt)
}
#[inline(never)]
fn level_5(x: u32) -> (u32, Backtrace) {
    let (y, bt) = level_6(x);
    (black_box(y + 1), bt)
}
#[inline(never)]
fn level_4(x: u32) -> (u32, Backtrace) {
    let (y, bt) = level_5(x);
    (black_box(y + 1), bt)
}
#[inline(never)]
fn level_3(x: u32) -> (u32, Backtrace) {
    let (y, bt) = level_4(x);
    (black_box(y + 1), bt)
}
#[inline(never)]
fn level_2(x: u32) -> (u32, Backtrace) {
    let (y, bt) = level_3(x);
    (black_box(y + 1), bt)
}
#[inline(never)]
fn level_1(x: u32) -> (u32, Backtrace) {
    let (y, bt) = level_2(x);
    (black_box(y + 1), bt)
}

fn main() {
    let (sum, bt) = level_1(0);
    println!("Sum: {} (prevents optimization)\n", sum);

    println!("=== All frames with symbols ===\n");
    let mut app_frames = 0;
    for (i, frame) in bt.frames().iter().enumerate() {
        for sym in frame.symbols() {
            if let Some(name) = sym.name() {
                let name_str = format!("{}", name);
                let is_app = name_str.contains("backtrace_frame_count")
                    && (name_str.contains("level_") || name_str.contains("main"));
                if is_app {
                    app_frames += 1;
                    println!("{:2}. [APP] {}", i, name_str);
                }
            }
        }
    }

    println!("\n=== Summary ===");
    println!("Total frames: {}", bt.frames().len());
    println!("App frames (level_N + main): {}", app_frames);
}
