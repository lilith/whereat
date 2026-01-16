//! Demonstrates enhanced trace display with AtCrateInfo and GitHub links.

use errat::{At, ResultAtExt, at};

// Required for at!() macro - defines __errat_crate_info() getter
errat::at_crate_info_static!();

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

fn fetch_user(id: u64) -> Result<(), At<AppError>> {
    if id == 0 {
        // at!() captures crate info automatically
        return Err(at!(AppError::Unauthorized));
    }
    Err(at!(AppError::NotFound {
        resource: format!("user/{}", id),
    }))
}

fn process_request(user_id: u64) -> Result<(), At<AppError>> {
    fetch_user(user_id).at_str("fetching user data")?;
    Ok(())
}

fn handle_api_call() -> Result<(), At<AppError>> {
    process_request(42).at_str("processing API request")
}

fn main() {
    let err = handle_api_call().unwrap_err();

    println!("=== Standard Debug output ===\n");
    println!("{:?}", err);

    println!("\n=== Enhanced output with AtCrateInfo ===\n");
    println!("{}", err.display_with_meta());

    println!("=== How AtCrateInfo works ===");
    println!(
        "
When you use at!() to create errors, it automatically:
1. Captures the crate name, repo URL, and git commit
2. Stores this as AtCrateInfo in the trace
3. display_with_meta() uses this to generate GitHub links

The at_crate_info_static!() macro defines a getter that captures:
"
    );

    let info = __errat_crate_info();
    println!("  - name: {}", info.name());
    println!("  - module: {}", info.module());
    if let Some(repo) = info.repo() {
        println!("  - repo: {}", repo);
    }
    if let Some(commit) = info.commit() {
        println!("  - commit: {}", commit);
    }

    println!(
        "
For cross-crate error handling, use at_crate!() macro
at crate boundaries to switch the repository used for links.
"
    );
}
