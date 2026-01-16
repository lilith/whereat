//! Examples of errat usage patterns - good, bad, and ugly.
//!
//! Run with: cargo run --example patterns
//!
//! This example demonstrates:
//! - Correct inner/outer At<E> patterns
//! - Common pitfalls and how to avoid them
//! - Performance implications of different approaches

use core::fmt;
use errat::{At, ResultAtExt, ResultStartAtExt, at};

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug)]
enum AppError {
    Io(std::io::Error),
    Parse(String),
    NotFound { key: String },
    Validation(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "I/O error: {}", e),
            AppError::Parse(msg) => write!(f, "parse error: {}", msg),
            AppError::NotFound { key } => write!(f, "not found: {}", key),
            AppError::Validation(msg) => write!(f, "validation error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Io(e) => Some(e),
            _ => None,
        }
    }
}

// =============================================================================
// GOOD PATTERNS
// =============================================================================

mod good_patterns {
    use super::*;

    /// Pattern 1: Origin point creates At<E>, callers extend with .at()
    ///
    /// This is the recommended pattern for most code.
    pub fn inner_creates_at() -> Result<String, At<AppError>> {
        // Error originates here - use at() to wrap
        Err(at(AppError::NotFound {
            key: "config".into(),
        }))
    }

    pub fn middle_extends_at() -> Result<String, At<AppError>> {
        // Just extend the trace - don't re-wrap!
        inner_creates_at().at()
    }

    pub fn outer_extends_at() -> Result<String, At<AppError>> {
        middle_extends_at().at()
    }

    /// Pattern 2: Add context without new location using at_str/at_string
    pub fn with_context() -> Result<String, At<AppError>> {
        // at_str adds context to the LAST location, not a new one
        inner_creates_at().at_str("while loading config")
    }

    /// Pattern 3: Explicit .at() for new location, then context
    pub fn location_then_context() -> Result<String, At<AppError>> {
        inner_creates_at()
            .at() // new location
            .at_str("in outer handler") // context on that location
    }

    /// Pattern 4: Converting external errors with start_at
    pub fn wrap_external_error() -> Result<String, At<AppError>> {
        let io_result: Result<String, std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));

        // Convert std::io::Error to AppError, then start tracing
        io_result
            .map_err(AppError::Io)
            .start_at()
            .at_str("reading config file")
    }

    /// Pattern 5: Late tracing when you don't control the origin
    pub fn late_tracing() -> Result<String, At<AppError>> {
        fn untraced_library() -> Result<String, AppError> {
            Err(AppError::Parse("unexpected token".into()))
        }

        // start_at_late() adds a [...] marker to show trace started late
        untraced_library()
            .start_at_late()
            .at_str("parsing user input")
    }

    /// Pattern 6: Multiple contexts on same location
    pub fn rich_context() -> Result<String, At<AppError>> {
        #[allow(dead_code)]
        #[derive(Debug)]
        struct RequestContext {
            user_id: u64,
            request_id: String,
        }

        // All context attaches to the single location from at()
        Err(at(AppError::Validation("invalid email".into()))
            .at_str("validating user profile")
            .at_debug(|| RequestContext {
                user_id: 42,
                request_id: "req-123".into(),
            })
            .at_string(|| format!("at {}", chrono_placeholder())))
    }

    fn chrono_placeholder() -> &'static str {
        "2024-01-15T10:30:00Z"
    }
}

// =============================================================================
// BAD PATTERNS (WASTEFUL)
// =============================================================================

mod bad_patterns {
    use super::*;

    /// Anti-pattern 1: Nested At<At<E>> - double wrapping
    ///
    /// This compiles but wastes memory by creating two traces.
    #[allow(dead_code)]
    pub fn double_wrapped() -> Result<String, At<At<AppError>>> {
        fn inner() -> Result<String, At<AppError>> {
            Err(at(AppError::NotFound { key: "x".into() }))
        }

        // BAD: wrapping an already-wrapped error
        Err(at(inner().unwrap_err()))
    }

    /// Anti-pattern 2: Using at() when you meant at_str()
    ///
    /// Creates unnecessary locations when you just want context.
    #[allow(dead_code)]
    pub fn at_instead_of_at_str() -> Result<String, At<AppError>> {
        // BAD: Creates 3 locations instead of 1 with 2 contexts
        Err(at(AppError::NotFound { key: "y".into() }))
            .at() // unnecessary new location
            .at_str("first context")
            .at() // unnecessary new location
            .at_str("second context")
    }

    /// Better version of the above
    #[allow(dead_code)]
    pub fn correct_multiple_contexts() -> Result<String, At<AppError>> {
        // GOOD: 1 location with 2 contexts
        Err(at(AppError::NotFound { key: "y".into() })
            .at_str("first context")
            .at_str("second context"))
    }

    /// Anti-pattern 3: Calling at() on every line in tight loop
    #[allow(dead_code)]
    pub fn at_in_tight_loop() -> Result<Vec<i32>, At<AppError>> {
        let items = [1, 2, -3, 4, 5];
        let mut results = Vec::new();

        for item in items {
            // BAD in hot path: at() allocates on every error
            if item < 0 {
                return Err(at(AppError::Validation(format!("negative: {}", item))));
            }
            results.push(item * 2);
        }

        Ok(results)
    }

    /// Better: validate all first, then create error once
    #[allow(dead_code)]
    pub fn validate_then_error() -> Result<Vec<i32>, At<AppError>> {
        let items = [1, 2, -3, 4, 5];

        // Validate all items first
        if let Some(bad) = items.iter().find(|&&x| x < 0) {
            // Single error creation
            return Err(at(AppError::Validation(format!("negative: {}", bad))));
        }

        Ok(items.iter().map(|x| x * 2).collect())
    }
}

// =============================================================================
// UGLY PATTERNS (SOMETIMES NECESSARY)
// =============================================================================

mod ugly_patterns {
    use super::*;

    /// Sometimes you need to handle both traced and untraced errors.
    ///
    /// This is ugly but sometimes necessary at API boundaries.
    #[allow(dead_code)]
    pub fn mixed_error_handling() -> Result<String, At<AppError>> {
        fn traced_fn() -> Result<String, At<AppError>> {
            Err(at(AppError::NotFound { key: "a".into() }))
        }

        fn untraced_fn() -> Result<String, AppError> {
            Err(AppError::Parse("bad".into()))
        }

        // Handle traced error
        if let Err(e) = traced_fn() {
            return Err(e.at_str("from traced"));
        }

        // Handle untraced error
        if let Err(e) = untraced_fn() {
            return Err(at(e).at_str("from untraced"));
        }

        Ok("ok".into())
    }

    /// When you need to inspect the error before deciding on context.
    #[allow(dead_code)]
    pub fn conditional_context() -> Result<String, At<AppError>> {
        fn inner() -> Result<String, At<AppError>> {
            Err(at(AppError::NotFound { key: "z".into() }))
        }

        inner().map_err(|e| {
            // Inspect error to decide on context
            match e.error() {
                AppError::NotFound { key } => {
                    let key = key.clone();
                    e.at_string(move || format!("missing key: {}", key))
                }
                _ => e.at_str("unknown error"),
            }
        })
    }
}

// =============================================================================
// MAIN - Demonstrate patterns
// =============================================================================

fn main() {
    println!("=== GOOD PATTERNS ===\n");

    // Pattern 1: Proper trace extension
    println!("Pattern 1: Inner creates At, outer extends with .at()");
    if let Err(e) = good_patterns::outer_extends_at() {
        println!("Error: {}", e);
        println!("Debug:\n{:?}\n", e);
    }

    // Pattern 2: Context without new location
    println!("Pattern 2: Add context with at_str (no new location)");
    if let Err(e) = good_patterns::with_context() {
        println!("Error: {}", e);
        println!("Trace len: {} (should be 1)\n", e.trace_len());
    }

    // Pattern 3: Explicit location then context
    println!("Pattern 3: Explicit .at() then .at_str()");
    if let Err(e) = good_patterns::location_then_context() {
        println!("Error: {}", e);
        println!("Trace len: {} (should be 2)\n", e.trace_len());
    }

    // Pattern 4: Wrapping external errors
    println!("Pattern 4: Converting external errors with start_at");
    if let Err(e) = good_patterns::wrap_external_error() {
        println!("Error: {}", e);
        println!("Debug:\n{:?}\n", e);
    }

    // Pattern 5: Late tracing
    println!("Pattern 5: Late tracing with start_at_late");
    if let Err(e) = good_patterns::late_tracing() {
        println!("Debug:\n{:?}\n", e);
    }

    // Pattern 6: Rich context
    println!("Pattern 6: Multiple contexts on same location");
    if let Err(e) = good_patterns::rich_context() {
        println!("Error: {}", e);
        println!("Trace len: {} (should be 1)", e.trace_len());
        println!("Context count: {}\n", e.contexts().count());
        println!("Debug:\n{:?}\n", e);
    }

    println!("\n=== COMPARING GOOD vs BAD ===\n");

    // Compare at_instead_of_at_str vs correct_multiple_contexts
    println!("Bad: at() when you meant at_str()");
    if let Err(e) = bad_patterns::at_instead_of_at_str() {
        println!("  Trace len: {} (wasteful!)", e.trace_len());
    }

    println!("Good: at_str() for context");
    if let Err(e) = bad_patterns::correct_multiple_contexts() {
        println!("  Trace len: {} (efficient)", e.trace_len());
    }

    println!("\n=== PERFORMANCE NOTES ===");
    println!(
        "
- Happy path (no errors): Near-zero overhead (~0.2ns)
- Error creation (at()): ~23ns (dominated by allocation)
- Per context (at_str): ~23ns additional
- Per trace level (.at()): ~6.5ns additional
- Hot loops with 100% errors: 133x slower than plain Result

Guidelines:
1. Use at() at error origin only
2. Use .at() to extend trace at call boundaries
3. Use .at_str() for context (doesn't create new location)
4. Avoid at() in tight loops - validate first, error once
5. Use start_at_late() when wrapping untraced errors
"
    );
}
