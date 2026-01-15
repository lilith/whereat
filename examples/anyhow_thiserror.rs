//! Demonstrates errat integration with thiserror and anyhow.
//!
//! Run with: cargo run --example anyhow_thiserror --features std

// Note: This example requires std feature and external crates
// For now, we'll simulate what the integration would look like

use errat::{At, ResultExt, Traceable};

// ============================================================================
// Simulating thiserror-style errors
// ============================================================================

/// Error type similar to what thiserror would generate
#[derive(Debug)]
#[allow(dead_code)]
enum AppError {
    Io(std::io::Error),
    Parse(String),
    NotFound { resource: String },
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "I/O error: {}", e),
            AppError::Parse(msg) => write!(f, "parse error: {}", msg),
            AppError::NotFound { resource } => write!(f, "{} not found", resource),
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

// ============================================================================
// Example 1: Wrapping thiserror-style error with errat
// ============================================================================

fn read_config_file(path: &str) -> Result<String, At<AppError>> {
    // Simulate file not found
    Err(AppError::NotFound {
        resource: path.to_string(),
    }
    .start_at())
}

fn load_config() -> Result<String, At<AppError>> {
    read_config_file("/etc/app.conf").at_message("loading application config")
}

fn init_app() -> Result<(), At<AppError>> {
    let _config = load_config().at_message("during initialization")?;
    Ok(())
}

// ============================================================================
// Example 2: Converting between errat and anyhow-style
// ============================================================================

/// Simulating anyhow::Error (boxed trait object)
type AnyError = Box<dyn std::error::Error + Send + Sync>;

/// Convert At<E> to boxed error (like anyhow would store it)
fn traced_to_any<E: std::error::Error + Send + Sync + 'static>(err: At<E>) -> AnyError {
    Box::new(err)
}

/// Wrap an anyhow-style error with errat tracing
fn any_to_traced(err: AnyError) -> At<AnyError> {
    errat::at(err)
}

// ============================================================================
// Example 3: Nested error chains
// ============================================================================

fn inner_operation() -> Result<(), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        "database unavailable",
    ))
}

fn middle_layer() -> Result<(), At<AppError>> {
    inner_operation()
        .map_err(AppError::Io)
        .map_err(|e| e.start_at())
        .at_message("connecting to database")
}

fn outer_layer() -> Result<(), At<AppError>> {
    middle_layer().at_message("in business logic")
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    println!("=== Example 1: thiserror-style with errat ===\n");
    if let Err(e) = init_app() {
        println!("{:?}", e);
    }

    println!("\n=== Example 2: Converting to boxed error ===\n");
    if let Err(e) = init_app() {
        let boxed: AnyError = traced_to_any(e);
        println!("Boxed error: {}", boxed);

        // Can wrap it again with errat
        let retraced = any_to_traced(boxed);
        println!("\nRe-traced:");
        println!("{:?}", retraced);
    }

    println!("\n=== Example 3: Nested error with source chain ===\n");
    if let Err(e) = outer_layer() {
        println!("{:?}", e);

        // Access the error chain
        println!("\nError chain:");
        let mut current: Option<&dyn std::error::Error> = Some(e.error());
        while let Some(err) = current {
            println!("  - {}", err);
            current = err.source();
        }
    }

    println!("\n=== Summary ===");
    println!(
        "
errat works well with thiserror-style errors:
- Wrap any error with .start_at() to start collecting locations
- Use .at_message() to add context as errors propagate
- At<E> implements Error, so it can be boxed like anyhow
- The error source chain is preserved through At<E>

Key patterns:
- thiserror defines the error types
- errat adds location tracking on top
- Can convert to Box<dyn Error> for anyhow-style usage
"
    );
}
