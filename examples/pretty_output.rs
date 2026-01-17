//! Demonstrates pretty formatters for error traces.
//!
//! Run with: cargo run --example pretty_output --features "_termcolor,_html"

use whereat::{At, ErrorAtExt, ResultAtExt};

#[derive(Debug)]
#[allow(dead_code)]
enum AppError {
    NotFound,
    InvalidInput(String),
}

impl core::fmt::Display for AppError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AppError::NotFound => write!(f, "not found"),
            AppError::InvalidInput(s) => write!(f, "invalid input: {}", s),
        }
    }
}

impl std::error::Error for AppError {}

fn find_user(id: u64) -> Result<String, At<AppError>> {
    if id == 0 {
        return Err(AppError::InvalidInput("user ID cannot be zero".into()).start_at());
    }
    Err(AppError::NotFound.start_at())
}

fn get_user_profile(id: u64) -> Result<String, At<AppError>> {
    find_user(id).at_str("looking up user profile")?;
    Ok("profile".into())
}

fn handle_request(id: u64) -> Result<String, At<AppError>> {
    get_user_profile(id).at_str("handling user request")
}

fn main() {
    let err = handle_request(42).unwrap_err();

    println!("=== Standard Debug Output ===\n");
    println!("{:?}", err);

    #[cfg(feature = "_termcolor")]
    {
        println!("\n=== Colored Terminal Output ===\n");
        println!("{}", err.display_color());
    }

    #[cfg(feature = "_html")]
    {
        println!("\n=== HTML Output ===\n");
        println!("{}", err.display_html());

        println!("\n=== HTML Output with Styles ===\n");
        println!("{}", err.display_html_styled());
    }

    #[cfg(not(any(feature = "_termcolor", feature = "_html")))]
    {
        println!("\n(Run with --features \"_termcolor,_html\" to see pretty output)");
    }
}
