//! Demonstrates errat with manual error type setup.
//!
//! Run with: cargo run --example derive_macro --features std

use errat::{At, ResultExt, at, crate_info};

#[derive(Debug)]
#[allow(dead_code)]
enum AppError {
    NotFound(String),
    InvalidInput(String),
    Io(std::io::Error),
    Unavailable { name: String, code: u32 },
    Internal,
}

impl core::fmt::Display for AppError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AppError::NotFound(s) => write!(f, "not found: {}", s),
            AppError::InvalidInput(s) => write!(f, "invalid input: {}", s),
            AppError::Io(e) => write!(f, "io error: {}", e),
            AppError::Unavailable { name, code } => {
                write!(f, "resource '{}' is unavailable (code: {})", name, code)
            }
            AppError::Internal => write!(f, "internal error"),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err)
    }
}

fn find_user(id: u64) -> Result<String, At<AppError>> {
    if id == 0 {
        // Use at!() macro for crate-aware error creation
        return Err(at!(AppError::NotFound(format!("user/{}", id))));
    }
    Ok(format!("User {}", id))
}

fn validate_input(s: &str) -> Result<(), At<AppError>> {
    if s.is_empty() {
        return Err(at!(AppError::InvalidInput("empty string".into())));
    }
    Ok(())
}

fn read_config() -> Result<String, At<AppError>> {
    let err = std::io::Error::new(std::io::ErrorKind::NotFound, "config.toml not found");
    Err(at!(AppError::from(err)))
}

fn process_request(user_id: u64, input: &str) -> Result<String, At<AppError>> {
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

    println!("\n=== Example 3: From impl ===\n");
    let err = read_config().unwrap_err();
    println!("{:?}", err);

    println!("\n=== Example 4: Named fields in error ===\n");
    let err = at!(AppError::Unavailable {
        name: "database".into(),
        code: 503,
    });
    println!("Display: {}", err);
    println!("Debug:\n{:?}", err);

    println!("\n=== Example 5: Enhanced output with display_with_meta ===\n");
    let err = process_request(0, "test").unwrap_err();
    println!("{}", err.display_with_meta());

    println!("\n=== Example 6: CrateInfo macro ===\n");
    let info = crate_info!();
    println!("Crate: {}", info.name);
    println!("Module: {}", info.module);
    if let Some(repo) = info.repo {
        println!("Repo: {}", repo);
    }
}
