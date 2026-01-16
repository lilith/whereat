//! # errat - Top 5 Use Cases
//!
//! Run with: `cargo run --example use_cases`

#![allow(dead_code)]

use errat::{At, ErrorAtExt, ResultAtExt, ResultStartAtExt, at, at_crate};

// Required for at!() and at_crate!() macros - defines __ERRAT_CRATE_INFO
errat::define_at_crate_info!();

// ============================================================================
// Use Case 1: Basic Error Propagation
// ============================================================================
//
// The most common pattern: propagate errors with location tracking.

mod use_case_1 {
    use super::*;

    #[derive(Debug)]
    pub enum AppError {
        NotFound,
        InvalidInput(String),
    }

    impl core::fmt::Display for AppError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                Self::NotFound => write!(f, "not found"),
                Self::InvalidInput(s) => write!(f, "invalid: {}", s),
            }
        }
    }

    impl core::error::Error for AppError {}

    // Layer 1: Origin of error
    fn query_db(id: u64) -> Result<String, At<AppError>> {
        if id == 0 {
            return Err(AppError::InvalidInput("id cannot be zero".into()).start_at());
        }
        Err(AppError::NotFound.start_at())
    }

    // Layer 2: Adds context
    fn get_user(id: u64) -> Result<String, At<AppError>> {
        query_db(id).at_str("querying database")?;
        Ok("User".into())
    }

    // Layer 3: More context
    pub fn handle_request(id: u64) -> Result<String, At<AppError>> {
        get_user(id).at_string(|| format!("fetching user {}", id))
    }

    pub fn demo() {
        println!("=== Use Case 1: Basic Error Propagation ===\n");

        let err = handle_request(42).unwrap_err();
        println!("{:?}", err);
        println!();
    }
}

// ============================================================================
// Use Case 2: Wrapping External/std Errors
// ============================================================================
//
// Convert errors from std or external crates into traced errors.

mod use_case_2 {
    use super::*;
    use std::io;

    // Wrap std::io::Error
    fn read_config(path: &str) -> Result<String, At<io::Error>> {
        std::fs::read_to_string(path)
            .start_at()
            .at_str("reading config file")
    }

    // Chain with more context
    pub fn load_settings() -> Result<String, At<io::Error>> {
        read_config("/nonexistent/config.toml").at_string(|| "loading application settings".into())
    }

    pub fn demo() {
        println!("=== Use Case 2: Wrapping External Errors ===\n");

        let err = load_settings().unwrap_err();
        println!("{:?}", err);
        println!();
    }
}

// ============================================================================
// Use Case 3: Cross-Crate Boundaries with GitHub Links
// ============================================================================
//
// Mark crate boundaries for accurate source links in multi-crate projects.

mod use_case_3 {
    use super::*;

    // Simulated "dependency crate"
    mod dependency {
        use super::*;

        #[derive(Debug)]
        pub struct DepError(pub &'static str);

        impl core::fmt::Display for DepError {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "dependency error: {}", self.0)
            }
        }

        impl core::error::Error for DepError {}

        pub fn dep_function() -> Result<(), At<DepError>> {
            Err(at!(DepError("something went wrong")))
        }
    }

    // Our crate wraps the dependency
    pub fn my_function() -> Result<(), At<dependency::DepError>> {
        // at_crate! marks that we're crossing into our crate
        at_crate!(dependency::dep_function())?;
        Ok(())
    }

    pub fn demo() {
        println!("=== Use Case 3: Cross-Crate Boundaries ===\n");

        let err = my_function().unwrap_err();

        // display_with_meta() shows GitHub links when AtCrateInfo has repo+commit
        println!("{}", err.display_with_meta());
        println!();
    }
}

// ============================================================================
// Use Case 4: Typed AtContext for Structured Logging
// ============================================================================
//
// Attach structured data for later retrieval (e.g., for JSON logging).

mod use_case_4 {
    use super::*;

    #[derive(Debug)]
    pub struct RequestContext {
        pub user_id: u64,
        pub endpoint: &'static str,
        pub trace_id: &'static str,
    }

    #[derive(Debug)]
    pub enum ApiError {
        Unauthorized,
        RateLimited,
    }

    impl core::fmt::Display for ApiError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                Self::Unauthorized => write!(f, "unauthorized"),
                Self::RateLimited => write!(f, "rate limited"),
            }
        }
    }

    impl core::error::Error for ApiError {}

    fn check_auth(_user_id: u64) -> Result<(), At<ApiError>> {
        Err(ApiError::Unauthorized.start_at())
    }

    pub fn handle_api_request(ctx: RequestContext) -> Result<(), At<ApiError>> {
        check_auth(ctx.user_id)
            // at_debug stores typed data, retrievable via downcast_ref
            .map_err(|e| e.at_debug(move || ctx))
    }

    pub fn demo() {
        println!("=== Use Case 4: Typed AtContext ===\n");

        let ctx = RequestContext {
            user_id: 42,
            endpoint: "/api/admin",
            trace_id: "abc-123",
        };

        let err = handle_api_request(ctx).unwrap_err();
        println!("{:?}", err);

        // Later: extract typed context for structured logging
        for context in err.contexts() {
            if let Some(req) = context.downcast_ref::<RequestContext>() {
                println!(
                    "\nExtracted context: user_id={}, trace_id={}",
                    req.user_id, req.trace_id
                );
            }
        }
        println!();
    }
}

// ============================================================================
// Use Case 5: Late Tracing for Legacy Code
// ============================================================================
//
// When integrating with code that doesn't use errat, mark the entry point.

mod use_case_5 {
    use super::*;

    // Simulated legacy code that returns plain errors
    mod legacy {
        pub fn old_function() -> Result<(), &'static str> {
            Err("legacy error message")
        }
    }

    // Wrapper that starts tracing late
    pub fn wrap_legacy() -> Result<(), At<&'static str>> {
        legacy::old_function()
            .start_at_late() // Marks that earlier frames were skipped
            .at_str("calling legacy code")
    }

    // Alternative: direct construction with at() + at_skipped_frames()
    pub fn wrap_legacy_alt() -> At<&'static str> {
        match legacy::old_function() {
            Ok(()) => unreachable!(),
            Err(e) => at(e).at_skipped_frames(),
        }
    }

    pub fn demo() {
        println!("=== Use Case 5: Late Tracing (Legacy Code) ===\n");

        let err = wrap_legacy().unwrap_err();
        println!("With start_at_late():");
        println!("{:?}", err);

        println!("\nWith at().at_skipped_frames():");
        let err = wrap_legacy_alt();
        println!("{:?}", err);
        println!();
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    use_case_1::demo();
    use_case_2::demo();
    use_case_3::demo();
    use_case_4::demo();
    use_case_5::demo();

    println!("=== Summary ===\n");
    println!("1. Basic:      .start_at() + .at() / .at_str() / .at_string()");
    println!("2. External:   .start_at() / .trace_str() to wrap non-At errors");
    println!("3. Cross-crate: at!() + at_crate!() for GitHub links");
    println!("4. Typed ctx:  .at_debug() / .at_data() + .downcast_ref()");
    println!("5. Legacy:     .start_at_late() / at().at_skipped_frames() for [...]");
}
