//! Compare display quality of different error tracing approaches
//! Run: cargo run --example display_quality
//! With backtrace: RUST_BACKTRACE=1 cargo run --example display_quality

use std::io;
use std::panic::catch_unwind;

// ============================================================================
// Nested error scenario: config loading -> file parsing -> IO error
// ============================================================================

// Inner error (IO level)
fn read_config_file(path: &str) -> io::Result<String> {
    // Simulate IO error
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("config file not found: {}", path),
    ))
}

// ============================================================================
// 1. BACKTRACE CRATE - manual capture
// ============================================================================

mod with_backtrace {
    use super::*;

    #[derive(Debug)]
    pub struct ConfigError {
        pub message: String,
        pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
        pub backtrace: backtrace::Backtrace,
        pub context: Vec<String>,
    }

    impl std::fmt::Display for ConfigError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.message)?;
            for ctx in &self.context {
                write!(f, "\n  context: {}", ctx)?;
            }
            if let Some(ref src) = self.source {
                write!(f, "\n  caused by: {}", src)?;
            }
            Ok(())
        }
    }

    impl std::error::Error for ConfigError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.source
                .as_ref()
                .map(|e| e.as_ref() as &dyn std::error::Error)
        }
    }

    impl ConfigError {
        pub fn new(message: impl Into<String>) -> Self {
            Self {
                message: message.into(),
                source: None,
                backtrace: backtrace::Backtrace::new(),
                context: Vec::new(),
            }
        }

        pub fn with_source(
            mut self,
            source: impl std::error::Error + Send + Sync + 'static,
        ) -> Self {
            self.source = Some(Box::new(source));
            self
        }

        pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
            self.context.push(ctx.into());
            self
        }
    }

    #[inline(never)]
    pub fn parse_config(content: &str) -> Result<(), ConfigError> {
        if content.is_empty() {
            return Err(ConfigError::new("empty config file")
                .with_context(format!("content length: {}", content.len())));
        }
        Ok(())
    }

    #[inline(never)]
    pub fn load_config(path: &str) -> Result<(), ConfigError> {
        let content = read_config_file(path).map_err(|e| {
            ConfigError::new("failed to read config")
                .with_source(e)
                .with_context(format!("path: {}", path))
        })?;
        parse_config(&content).map_err(|e| e.with_context("during config loading"))
    }

    #[inline(never)]
    pub fn initialize_app() -> Result<(), ConfigError> {
        load_config("/etc/myapp/config.toml")
            .map_err(|e| e.with_context("app initialization failed"))
    }
}

// ============================================================================
// 2. PANIC + CATCH_UNWIND
// ============================================================================

mod with_panic {
    use super::*;

    #[inline(never)]
    pub fn parse_config(content: &str) {
        if content.is_empty() {
            panic!("empty config file (content length: {})", content.len());
        }
    }

    #[inline(never)]
    pub fn load_config(path: &str) {
        match read_config_file(path) {
            Ok(content) => parse_config(&content),
            Err(e) => panic!("failed to read config at '{}': {}", path, e),
        }
    }

    #[inline(never)]
    pub fn initialize_app() {
        load_config("/etc/myapp/config.toml");
    }
}

// ============================================================================
// 3. ERRAT - #[track_caller] with context
// ============================================================================

mod with_errat {
    use super::*;
    use errat::{At, ResultAtExt, at};

    #[derive(Debug)]
    pub enum ConfigError {
        Io(io::Error),
        Parse(String),
    }

    impl std::fmt::Display for ConfigError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Io(e) => write!(f, "IO error: {}", e),
                Self::Parse(msg) => write!(f, "parse error: {}", msg),
            }
        }
    }

    impl std::error::Error for ConfigError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                Self::Io(e) => Some(e),
                Self::Parse(_) => None,
            }
        }
    }

    #[inline(never)]
    pub fn parse_config(content: &str) -> Result<(), At<ConfigError>> {
        if content.is_empty() {
            return Err(at(ConfigError::Parse("empty config file".into()))
                .at_string(|| format!("content length: {}", content.len())));
        }
        Ok(())
    }

    #[inline(never)]
    pub fn load_config(path: &str) -> Result<(), At<ConfigError>> {
        let content = read_config_file(path)
            .map_err(|e| at(ConfigError::Io(e)).at_string(|| format!("path: {}", path)))?;
        parse_config(&content).at().at_str("during config loading")
    }

    #[inline(never)]
    pub fn initialize_app() -> Result<(), At<ConfigError>> {
        load_config("/etc/myapp/config.toml")
            .at()
            .at_str("app initialization failed")
    }
}

// ============================================================================
// 4. ANYHOW - for comparison
// ============================================================================

mod with_anyhow {
    use super::*;
    use anyhow::{Context, Result};

    #[inline(never)]
    pub fn parse_config(content: &str) -> Result<()> {
        if content.is_empty() {
            anyhow::bail!("empty config file (content length: {})", content.len());
        }
        Ok(())
    }

    #[inline(never)]
    pub fn load_config(path: &str) -> Result<()> {
        let content = read_config_file(path)
            .with_context(|| format!("failed to read config at '{}'", path))?;
        parse_config(&content).context("during config loading")
    }

    #[inline(never)]
    pub fn initialize_app() -> Result<()> {
        load_config("/etc/myapp/config.toml").context("app initialization failed")
    }
}

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║         ERROR DISPLAY QUALITY COMPARISON                         ║");
    println!("║         Nested errors with context data                          ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    // =========================================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("1. BACKTRACE CRATE");
    println!("   Captures: Full native stack + manual context");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let err = with_backtrace::initialize_app().unwrap_err();
    println!("Display:\n{}\n", err);
    println!("Backtrace (first 20 frames):");
    let bt = format!("{:?}", err.backtrace);
    for line in bt.lines().take(20) {
        println!("  {}", line);
    }
    println!("  ...\n");

    // =========================================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "2. PANIC + CATCH_UNWIND (RUST_BACKTRACE={})",
        std::env::var("RUST_BACKTRACE").unwrap_or_else(|_| "unset".into())
    );
    println!("   Captures: Panic message only (backtrace to stderr if enabled)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Temporarily suppress panic output
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    #[allow(clippy::redundant_closure)] // Closure needed for UnwindSafe
    let result = catch_unwind(|| with_panic::initialize_app());

    std::panic::set_hook(prev_hook);

    match result {
        Err(payload) => {
            if let Some(s) = payload.downcast_ref::<&str>() {
                println!("Panic payload: {}\n", s);
            } else if let Some(s) = payload.downcast_ref::<String>() {
                println!("Panic payload: {}\n", s);
            }
            println!("Note: No nested error info, no context chain.");
            println!("      Backtrace only available via RUST_BACKTRACE=1 to stderr.\n");
        }
        Ok(_) => println!("No panic\n"),
    }

    // =========================================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "3. ANYHOW (RUST_BACKTRACE={})",
        std::env::var("RUST_BACKTRACE").unwrap_or_else(|_| "unset".into())
    );
    println!("   Captures: Error chain with context, optional backtrace");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let err = with_anyhow::initialize_app().unwrap_err();
    println!("Display: {}\n", err);
    println!("Debug (full chain):\n{:?}\n", err);

    // =========================================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("4. ERRAT (#[track_caller])");
    println!("   Captures: Source locations + context at each .at() call");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let err = with_errat::initialize_app().unwrap_err();

    println!("Display (message only):\n{}\n", err);

    println!("Debug (message + locations):\n{:?}\n", err);

    println!("full_trace() (message + locations + all context):");
    println!("{}\n", err.full_trace());

    // =========================================================================
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("SUMMARY: Display Quality Comparison");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!("| Feature                    | backtrace | panic | anyhow | errat |");
    println!("|----------------------------|-----------|-------|--------|-------|");
    println!("| Source file:line           | ✓ (noisy) | ✗     | ✗      | ✓     |");
    println!("| Function names             | ✓ (noisy) | ✗     | ✗      | ✗     |");
    println!("| Custom context strings     | manual    | ✗     | ✓      | ✓     |");
    println!("| Nested error chain         | manual    | ✗     | ✓      | ✓     |");
    println!("| Works without env var      | ✓         | ✗     | ✗      | ✓     |");
    println!("| Compact output             | ✗         | ✓     | ✓      | ✓     |");
    println!("| Shows YOUR code only       | ✗         | ✗     | ✗      | ✓     |");
    println!("| Per-error cost             | ~6µs      | ~1µs  | ~46ns  | ~25ns |");
    println!();
}
