//! Demonstrates errat stack traces in a realistic scenario.

use errat::{At, ResultExt, Traceable};

#[derive(Debug)]
#[allow(dead_code)]
enum AppError {
    Database(String),
    Validation(String),
    NotFound,
}

impl core::fmt::Display for AppError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AppError::Database(msg) => write!(f, "database error: {}", msg),
            AppError::Validation(msg) => write!(f, "validation error: {}", msg),
            AppError::NotFound => write!(f, "not found"),
        }
    }
}

impl std::error::Error for AppError {}

// Simulated database layer
mod db {
    use super::*;

    pub fn query_user(id: u64) -> Result<(), At<AppError>> {
        if id == 0 {
            return Err(AppError::Database("connection timeout".into()).start_at());
        }
        Err(AppError::NotFound.start_at())
    }
}

// Service layer
mod service {
    use super::*;

    pub fn get_user(id: u64) -> Result<(), At<AppError>> {
        db::query_user(id).at_message("querying database")
    }

    pub fn validate_user_access(user_id: u64, _resource: &str) -> Result<(), At<AppError>> {
        get_user(user_id).at_message("checking user exists")?;
        // More validation...
        Ok(())
    }
}

// Handler layer
mod handler {
    use super::*;

    pub fn handle_request(user_id: u64) -> Result<(), At<AppError>> {
        service::validate_user_access(user_id, "/admin").at_message("validating access")
    }
}

fn main() {
    println!("=== Example 1: Simple trace ===\n");
    let err = db::query_user(42).unwrap_err();
    println!("{:?}", err);

    println!("\n=== Example 2: Multi-layer trace ===\n");
    let err = handler::handle_request(42).unwrap_err();
    println!("{:?}", err);

    println!("\n=== Example 3: With typed context ===\n");
    #[derive(Debug)]
    #[allow(dead_code)]
    struct RequestContext {
        user_id: u64,
        endpoint: &'static str,
    }

    let err = handler::handle_request(42)
        .map_err(|e| {
            e.at_debug(|| RequestContext {
                user_id: 42,
                endpoint: "/admin",
            })
        })
        .unwrap_err();
    println!("{:?}", err);
}
