//! Demonstrates the TracedError derive macro.
//!
//! Run with: cargo run --example derive_macro --features derive,std

use errat::{ResultExt, Traceable, Traced, TracedError};

#[derive(Debug, TracedError)]
#[errat(repo = "https://github.com/imazen/errat", commit = "main")]
#[allow(dead_code)]
enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("io error: {0}")]
    #[from]
    Io(std::io::Error),

    #[error("resource '{name}' is unavailable (code: {code})")]
    Unavailable { name: String, code: u32 },

    #[error("internal error")]
    Internal,
}

fn find_user(id: u64) -> Result<String, Traced<AppError>> {
    if id == 0 {
        return Err(AppError::NotFound(format!("user/{}", id)).start_at());
    }
    Ok(format!("User {}", id))
}

fn validate_input(s: &str) -> Result<(), Traced<AppError>> {
    if s.is_empty() {
        return Err(AppError::InvalidInput("empty string".into()).start_at());
    }
    Ok(())
}

fn read_config() -> Result<String, Traced<AppError>> {
    // This uses the #[from] impl - io::Error converts to AppError::Io
    // Then we use .start_at() to wrap in Traced and capture location
    let err = std::io::Error::new(std::io::ErrorKind::NotFound, "config.toml not found");
    Err(AppError::from(err).start_at())
}

fn process_request(user_id: u64, input: &str) -> Result<String, Traced<AppError>> {
    validate_input(input).at_message("validating request input")?;
    let user = find_user(user_id).at_message("looking up user")?;
    Ok(format!(
        "Processed request for {} with input '{}'",
        user, input
    ))
}

fn main() {
    println!("=== Example 1: Basic traced error ===\n");
    let err = find_user(0).unwrap_err();
    println!("{:?}", err);

    println!("\n=== Example 2: Error with context messages ===\n");
    let err = process_request(0, "hello").unwrap_err();
    println!("{:?}", err);

    println!("\n=== Example 3: From impl with #[from] ===\n");
    let err = read_config().unwrap_err();
    println!("{:?}", err);

    println!("\n=== Example 4: Named fields in error ===\n");
    let err = AppError::Unavailable {
        name: "database".into(),
        code: 503,
    }
    .start_at();
    println!("Display: {}", err);
    println!("Debug:\n{:?}", err);

    println!("\n=== Example 5: ErrorMeta with display_with_meta ===\n");
    let err = process_request(0, "test").unwrap_err();
    println!("{}", err.display_with_meta());
}
