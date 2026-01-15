# User Feedback Log

## 2026-01-14

- User requested fallible allocations - trace ops should silently fail on OOM, error still propagates
- User clarified: Rust 1.92 try_* methods - checked and Box::try_new still unstable, using stable try_reserve for Vec/String
- User requested: no unsafe ever
- User confirmed name should stay "errat" (vs erred, errr)
- User requested: look at thiserror macros for formatting strings, add as TODO to README
