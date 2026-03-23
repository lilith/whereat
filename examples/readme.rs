//! The README Quick Start example — compiles and runs.
//!
//! Run with: cargo run --example readme

// once in lib.rs — enables GitHub links in traces
// For workspace crates: whereat::define_at_crate_info!(path = "crates/mylib/");
whereat::define_at_crate_info!();

use whereat::prelude::*; // At, at, ResultAtExt, ErrorAtExt

// --- Beat 1: create and propagate ---

#[derive(Debug)]
enum DbError {
    NotFound,
    #[allow(dead_code)]
    ConnectionFailed,
}

impl core::fmt::Display for DbError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DbError::NotFound => write!(f, "not found"),
            DbError::ConnectionFailed => write!(f, "connection failed"),
        }
    }
}
impl core::error::Error for DbError {}

fn get_user(id: u64) -> Result<String, At<DbError>> {
    if id == 0 {
        return Err(at!(DbError::NotFound)); // at!() starts the trace
    }
    Ok("alice".into())
}

fn get_email(id: u64) -> Result<String, At<DbError>> {
    let name = get_user(id).at()?; // .at()? adds this call site to the trace
    Ok(format!("{}@example.com", name))
}

// --- Beat 2: multiple error types ---

#[derive(Debug)]
enum ApiError {
    Db(DbError),
    #[allow(dead_code)]
    BadRequest(String),
}

impl core::fmt::Display for ApiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ApiError::Db(e) => write!(f, "database error: {e}"),
            ApiError::BadRequest(msg) => write!(f, "bad request: {msg}"),
        }
    }
}
impl core::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            ApiError::Db(e) => Some(e),
            _ => None,
        }
    }
}

type ApiResult<T> = Result<T, At<ApiError>>; // Result alias — recommended for every crate

fn handle_request(id: u64) -> ApiResult<String> {
    let email = get_email(id)
        .at() // new frame at this call site
        .at_str("looking up recipient") // context on that frame
        .map_err_at(ApiError::Db)?; // DbError → ApiError, trace preserved
    Ok(email)
}

fn api_endpoint(id: u64) -> ApiResult<String> {
    let result = handle_request(id).at()?; // propagate with location tracking
    Ok(result)
}

fn main() {
    let err = api_endpoint(0).unwrap_err();

    println!("=== Display (just the error message) ===");
    println!("{err}");

    println!("\n=== Debug (full trace) ===");
    println!("{err:?}");

    println!("\n=== full_trace() ===");
    println!("{}", err.full_trace());

    println!("\n=== Frame count: {} ===", err.frame_count());
}
