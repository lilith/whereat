//! Demonstrates ErrorMeta for enhanced trace display with GitHub links.

use errat::{ErrorMeta, ResultExt, Traceable, Traced};

#[derive(Debug)]
#[allow(dead_code)]
enum AppError {
    NotFound { resource: String },
    Unauthorized,
    DatabaseError(String),
}

impl core::fmt::Display for AppError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AppError::NotFound { resource } => write!(f, "{} not found", resource),
            AppError::Unauthorized => write!(f, "unauthorized access"),
            AppError::DatabaseError(msg) => write!(f, "database error: {}", msg),
        }
    }
}

// Implement ErrorMeta for enhanced trace output
impl ErrorMeta for AppError {
    fn crate_name(&self) -> Option<&'static str> {
        Some("my_app")
    }

    fn repo_url(&self) -> Option<&'static str> {
        // In real code, use env!("CARGO_PKG_REPOSITORY") or similar
        Some("https://github.com/imazen/errat")
    }

    fn git_commit(&self) -> Option<&'static str> {
        // In real code, capture at build time with build.rs
        // option_env!("GIT_COMMIT")
        Some("main") // Using branch name for demo
    }

    fn trace_summary(&self) -> Option<&str> {
        // Provide a cleaner summary than Debug format
        match self {
            AppError::NotFound { .. } => Some("Resource not found"),
            AppError::Unauthorized => Some("Access denied"),
            AppError::DatabaseError(_) => Some("Database operation failed"),
        }
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://docs.rs/errat")
    }
}

fn fetch_user(id: u64) -> Result<(), Traced<AppError>> {
    if id == 0 {
        return Err(AppError::Unauthorized.start_at());
    }
    Err(AppError::NotFound {
        resource: format!("user/{}", id),
    }
    .start_at())
}

fn process_request(user_id: u64) -> Result<(), Traced<AppError>> {
    fetch_user(user_id).at_message("fetching user data")?;
    Ok(())
}

fn handle_api_call() -> Result<(), Traced<AppError>> {
    process_request(42).at_message("processing API request")
}

fn main() {
    let err = handle_api_call().unwrap_err();

    println!("=== Standard Debug output ===\n");
    println!("{:?}", err);

    println!("\n=== Enhanced output with ErrorMeta ===\n");
    println!("{}", err.display_with_meta());

    println!("=== What ErrorMeta provides ===");
    println!(
        "
With ErrorMeta implemented, display_with_meta() adds:
- trace_summary(): Clean one-line error description
- crate_name(): Shows which crate the error comes from
- repo_url() + git_commit(): Clickable GitHub links for each location
- docs_url(): Link to documentation

This makes production error logs much more actionable!
"
    );
}
