//! Chained method for function name capture
//! Run: cargo run --example chained_fn_name
//!
//! This example demonstrates the built-in `.at_fn(|| {})` method which captures
//! both the source location (file:line:col) AND the function name at zero runtime cost.

use errat::{at, At, ResultAtExt};

// ============================================================================
// Example error type
// ============================================================================

#[derive(Debug)]
enum ConfigError {
    NotFound(String),
    ParseError(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "config not found: {}", path),
            Self::ParseError(msg) => write!(f, "parse error: {}", msg),
        }
    }
}

// ============================================================================
// Example usage with built-in .at_fn()
// ============================================================================

mod config {
    use super::*;

    pub fn load_file(path: &str) -> Result<String, At<ConfigError>> {
        Err(at(ConfigError::NotFound(path.to_string()))
            .at_fn(|| {}))  // <-- captures file:line + function name!
    }

    pub fn parse(content: &str) -> Result<(), At<ConfigError>> {
        if content.is_empty() {
            return Err(at(ConfigError::ParseError("empty".into()))
                .at_fn(|| {}));
        }
        Ok(())
    }

    pub fn load_and_parse(path: &str) -> Result<(), At<ConfigError>> {
        let content = load_file(path)
            .at_fn(|| {})?;  // <-- works on Result too!
        parse(&content)
            .at_fn(|| {})
    }
}

mod app {
    use super::*;

    pub fn initialize() -> Result<(), At<ConfigError>> {
        config::load_and_parse("/etc/app/config.toml")
            .at_fn(|| {})
            .at_str("during app initialization")
    }
}

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║         BUILT-IN .at_fn(|| {{}}) METHOD                            ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    // Show what the closure type looks like
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("CLOSURE TYPE NAME");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let closure = || {};
    println!("type_name of `|| {{}}` in main():");
    println!("  {:?}\n", std::any::type_name_of_val(&closure));

    // .at_fn(|| {}) now captures both location AND function name
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(".at_fn(|| {{}}) - File:line + function name (built-in)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let err = app::initialize().unwrap_err();
    println!("{}\n", err.full_trace());

    // API comparison
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("API COMPARISON");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!("| Method              | Captures                    | Usage              |");
    println!("|---------------------|-----------------------------|--------------------|");
    println!("| .at()               | file:line                   | .at()              |");
    println!("| .at_str(\"ctx\")      | context string              | .at_str(\"...\")     |");
    println!("| .at_fn(|| {{}})       | file:line + function name   | .at_fn(|| {{}})      |");
    println!();
    println!("All methods are zero-cost: type_name resolved at compile time!");
}
