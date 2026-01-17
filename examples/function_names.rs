//! Function name capture with built-in .at_fn() method
//! Run: cargo run --example function_names
//!
//! This example demonstrates the built-in `.at_fn(|| {})` method which captures
//! both the source location (file:line:col) AND the function name at zero runtime cost.
//!
//! The trick: closure types include their parent function name in their type_name.

use errat::{at, At, ResultAtExt};

/// Helper to show the type_name_of trick
#[inline(always)]
fn type_name_of<T>(_: T) -> &'static str {
    std::any::type_name::<T>()
}

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
// Functions using the built-in at_fn() method
// ============================================================================

mod config {
    use super::*;

    pub fn load_file(path: &str) -> Result<String, At<ConfigError>> {
        // Simulate not found
        Err(at(ConfigError::NotFound(path.to_string())).at_fn(|| {}))
    }

    pub fn parse(content: &str) -> Result<(), At<ConfigError>> {
        if content.is_empty() {
            return Err(at(ConfigError::ParseError("empty".into())).at_fn(|| {}));
        }
        Ok(())
    }

    pub fn load_and_parse(path: &str) -> Result<(), At<ConfigError>> {
        let content = load_file(path).at_fn(|| {})?;
        parse(&content).at_fn(|| {})
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
    println!("║         FUNCTION NAME CAPTURE WITH .at_fn(|| {{}})                 ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    // Show raw type_name_of output
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("RAW type_name_of() OUTPUT");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    fn dummy() {}
    println!("type_name_of(dummy)           = {:?}", type_name_of(dummy));
    println!("type_name_of(app::initialize) = {:?}", type_name_of(app::initialize));
    println!("type_name_of(config::load_file) = {:?}", type_name_of(config::load_file));

    // Nested fn trick
    fn f() {}
    let name = type_name_of(f);
    println!("\nNested fn trick in main():");
    println!("  type_name_of(f) = {:?}", name);
    println!("  Stripped        = {:?}", &name[..name.len() - 3]);

    // Closure type includes parent function
    let closure = || {};
    println!("\nClosure type in main():");
    println!("  type_name_of_val = {:?}", std::any::type_name_of_val(&closure));

    // Built-in at_fn approach
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(".at_fn(|| {{}}) - File:line + function name (built-in)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let err = app::initialize().unwrap_err();
    println!("full_trace():\n{}\n", err.full_trace());

    // Summary
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("COMPARISON");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!("| Approach       | Output                                    |");
    println!("|----------------|-------------------------------------------|");
    println!("| .at() only     | examples/function_names.rs:42:13          |");
    println!("| .at_fn(|| {{}}) | examples/function_names.rs:42:13          |");
    println!("|                |     in function_names::config::load_file  |");
    println!();
    println!("Cost: type_name_of() is resolved at compile time - ZERO runtime cost!");
}
