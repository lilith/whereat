//! Pretty formatters for error traces.
//!
//! This module provides colored terminal and HTML formatting for error traces.
//! These are internal/unstable features enabled via `_termcolor` and `_html` flags.

use crate::context::AtContext;
use crate::{At, AtCrateInfo};
use alloc::format;
use alloc::string::String;
use core::fmt::{self, Write as FmtWrite};

// ============================================================================
// Terminal Color Formatter
// ============================================================================

#[cfg(feature = "_termcolor")]
mod term {
    use super::*;
    use alloc::string::ToString;
    use owo_colors::OwoColorize;

    /// Wrapper for colored terminal display of `At<E>`.
    pub struct TermColorDisplay<'a, E> {
        pub(super) traced: &'a At<E>,
    }

    impl<E: fmt::Debug> fmt::Display for TermColorDisplay<'_, E> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            // Error header in red/bold
            write!(f, "{} ", "Error:".red().bold())?;
            writeln!(f, "{}", format!("{:?}", self.traced.error()).red())?;

            let Some(trace) = self.traced.trace_ref() else {
                return Ok(());
            };

            writeln!(f)?;

            // Track current crate for boundary display
            let mut current_crate: Option<&str> = trace.crate_info().map(|i| i.name());

            // Walk locations
            for (i, loc_opt) in trace.iter().enumerate() {
                // Check for crate boundary before showing location
                for context in trace.contexts_at(i) {
                    if let AtContext::Crate(info) = context {
                        let from = current_crate.unwrap_or("?");
                        let to = info.name();
                        write!(f, "    {} ", "───".dimmed())?;
                        write!(f, "{}", from.bright_blue())?;
                        write!(f, "{}", " (above) → ".dimmed())?;
                        write!(f, "{}", to.bright_blue())?;
                        write!(f, "{}", " (below)".dimmed())?;
                        writeln!(f, " {}", "───".dimmed())?;
                        current_crate = Some(to);
                    }
                }

                match loc_opt {
                    Some(loc) => {
                        // "at" in dim, location in cyan
                        write!(f, "    {} ", "at".dimmed())?;
                        write!(f, "{}", loc.file().cyan())?;
                        write!(f, "{}", ":".dimmed())?;
                        writeln!(f, "{}", loc.line().to_string().yellow())?;

                        // Contexts with corner prefix (skip crate boundaries, already shown)
                        for context in trace.contexts_at(i) {
                            if matches!(context, AtContext::Crate(_)) {
                                continue;
                            }
                            write!(f, "       {} ", "╰─".dimmed())?;
                            match context {
                                AtContext::Text(msg) => writeln!(f, "{}", msg.as_ref().green())?,
                                AtContext::FunctionName(name) => {
                                    write!(f, "{} ", "in".dimmed())?;
                                    writeln!(f, "{}", name.bright_blue())?
                                }
                                AtContext::Debug(t) => {
                                    writeln!(f, "{}", format!("{:?}", t).magenta())?
                                }
                                AtContext::Display(t) => {
                                    writeln!(f, "{}", format!("{}", t).magenta())?
                                }
                                AtContext::Error(e) => {
                                    write!(f, "{} ", "caused by:".dimmed())?;
                                    writeln!(f, "{}", format!("{}", e).red())?
                                }
                                AtContext::Crate(_) => unreachable!(),
                            }
                        }
                    }
                    None => {
                        writeln!(f, "    {}", "[...]".dimmed())?;
                    }
                }
            }

            Ok(())
        }
    }

    /// Wrapper for colored terminal display with metadata.
    pub struct TermColorMetaDisplay<'a, E> {
        pub(super) traced: &'a At<E>,
    }

    impl<E: fmt::Debug> fmt::Display for TermColorMetaDisplay<'_, E> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            // Error header in red/bold
            write!(f, "{} ", "Error:".red().bold())?;
            writeln!(f, "{}", format!("{:?}", self.traced.error()).red())?;

            let Some(trace) = self.traced.trace_ref() else {
                return Ok(());
            };

            // Show crate info if available
            if let Some(info) = trace.crate_info() {
                write!(f, "  {} ", "crate:".dimmed())?;
                writeln!(f, "{}", info.name().bright_blue())?;
            }

            writeln!(f)?;

            // Track current crate for boundary display
            let mut current_crate: Option<&str> = trace.crate_info().map(|i| i.name());

            // Build link template from crate info
            let mut link_template: Option<String> = trace.crate_info().and_then(build_link_base);

            // Walk locations
            for (i, loc_opt) in trace.iter().enumerate() {
                // Check for crate boundary context - display prominently and update link template
                for context in trace.contexts_at(i) {
                    if let AtContext::Crate(info) = context {
                        let from = current_crate.unwrap_or("?");
                        let to = info.name();
                        write!(f, "    {} ", "───".dimmed())?;
                        write!(f, "{}", from.bright_blue())?;
                        write!(f, "{}", " (above) → ".dimmed())?;
                        write!(f, "{}", to.bright_blue())?;
                        write!(f, "{}", " (below)".dimmed())?;
                        writeln!(f, " {}", "───".dimmed())?;
                        current_crate = Some(to);
                        link_template = build_link_base(info);
                    }
                }

                match loc_opt {
                    Some(loc) => {
                        write!(f, "    {} ", "at".dimmed())?;

                        // Show link if available
                        if let Some(ref template) = link_template {
                            let url = template
                                .replace("{file}", loc.file())
                                .replace("{line}", &loc.line().to_string());
                            // File as link (underlined)
                            write!(f, "{}", loc.file().cyan().underline())?;
                            write!(f, "{}", ":".dimmed())?;
                            write!(f, "{}", loc.line().to_string().yellow())?;
                            writeln!(f, " {}{}{}", "(".dimmed(), url.dimmed(), ")".dimmed())?;
                        } else {
                            write!(f, "{}", loc.file().cyan())?;
                            write!(f, "{}", ":".dimmed())?;
                            writeln!(f, "{}", loc.line().to_string().yellow())?;
                        }

                        // Contexts (skip crate boundaries, already shown)
                        for context in trace.contexts_at(i) {
                            if matches!(context, AtContext::Crate(_)) {
                                continue;
                            }
                            write!(f, "       {} ", "╰─".dimmed())?;
                            match context {
                                AtContext::Text(msg) => writeln!(f, "{}", msg.as_ref().green())?,
                                AtContext::FunctionName(name) => {
                                    write!(f, "{} ", "in".dimmed())?;
                                    writeln!(f, "{}", name.bright_blue())?
                                }
                                AtContext::Debug(t) => {
                                    writeln!(f, "{}", format!("{:?}", t).magenta())?
                                }
                                AtContext::Display(t) => {
                                    writeln!(f, "{}", format!("{}", t).magenta())?
                                }
                                AtContext::Error(e) => {
                                    write!(f, "{} ", "caused by:".dimmed())?;
                                    writeln!(f, "{}", format!("{}", e).red())?
                                }
                                AtContext::Crate(_) => unreachable!(),
                            }
                        }
                    }
                    None => {
                        writeln!(f, "    {}", "[...]".dimmed())?;
                    }
                }
            }

            Ok(())
        }
    }

    fn build_link_base(info: &AtCrateInfo) -> Option<String> {
        let repo = info.repo()?;
        let commit = info.commit()?;
        let path = info.crate_path().unwrap_or("");

        // Use the link format from crate info
        let link_format = info.link_format();

        let base = link_format
            .replace("{repo}", repo)
            .replace("{commit}", commit)
            .replace("{path}", path);

        Some(base)
    }
}

#[cfg(feature = "_termcolor")]
pub use term::{TermColorDisplay, TermColorMetaDisplay};

// ============================================================================
// HTML Formatter
// ============================================================================

#[cfg(feature = "_html")]
mod html {
    use super::*;
    use alloc::string::ToString;

    /// CSS styles for HTML error output (Catppuccin Mocha theme).
    pub const HTML_STYLES: &str = r#"
.whereat-error {
    font-family: 'SF Mono', 'Menlo', 'Monaco', 'Consolas', monospace;
    font-size: 13px;
    line-height: 1.5;
    background: #1e1e2e;
    color: #cdd6f4;
    padding: 16px;
    border-radius: 8px;
    overflow-x: auto;
}
.whereat-error .error-header {
    color: #f38ba8;
    font-weight: bold;
}
.whereat-error .crate-info {
    color: #6c7086;
    margin-bottom: 8px;
}
.whereat-error .crate-name {
    color: #89b4fa;
}
.whereat-error .location {
    margin-left: 16px;
}
.whereat-error .at-prefix {
    color: #6c7086;
}
.whereat-error .file {
    color: #89dceb;
}
.whereat-error .line {
    color: #f9e2af;
}
.whereat-error .context {
    margin-left: 28px;
    color: #6c7086;
}
.whereat-error .context-text {
    color: #a6e3a1;
}
.whereat-error .context-fn {
    color: #89b4fa;
}
.whereat-error .context-data {
    color: #cba6f7;
}
.whereat-error .context-error {
    color: #f38ba8;
}
.whereat-error .skip-marker {
    margin-left: 16px;
    color: #6c7086;
}
.whereat-error .crate-boundary {
    margin-left: 16px;
    color: #6c7086;
    margin-top: 4px;
    margin-bottom: 4px;
}
.whereat-error .crate-boundary .crate-name {
    color: #89b4fa;
    font-weight: 500;
}
.whereat-error a {
    color: inherit;
    text-decoration: underline;
    text-decoration-color: #6c7086;
}
.whereat-error a:hover {
    text-decoration-color: #89dceb;
}
"#;

    /// Wrapper for HTML display of `At<E>`.
    pub struct HtmlDisplay<'a, E> {
        pub(super) traced: &'a At<E>,
        pub(super) include_styles: bool,
    }

    impl<E: fmt::Debug> fmt::Display for HtmlDisplay<'_, E> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.include_styles {
                writeln!(f, "<style>{}</style>", HTML_STYLES)?;
            }

            writeln!(f, "<div class=\"whereat-error\">")?;

            // Error header
            write!(f, "<div class=\"error-header\">Error: ")?;
            write_html_escaped(f, &format!("{:?}", self.traced.error()))?;
            writeln!(f, "</div>")?;

            let Some(trace) = self.traced.trace_ref() else {
                writeln!(f, "</div>")?;
                return Ok(());
            };

            // Crate info
            if let Some(info) = trace.crate_info() {
                write!(f, "<div class=\"crate-info\">crate: ")?;
                write!(f, "<span class=\"crate-name\">")?;
                write_html_escaped(f, info.name())?;
                writeln!(f, "</span></div>")?;
            }

            // Track current crate for boundary display
            let mut current_crate: Option<&str> = trace.crate_info().map(|i| i.name());

            // Build link template
            let mut link_template: Option<String> = trace.crate_info().and_then(build_link_base);

            // Locations
            for (i, loc_opt) in trace.iter().enumerate() {
                // Check for crate boundary context - display prominently and update link template
                for context in trace.contexts_at(i) {
                    if let AtContext::Crate(info) = context {
                        let from = current_crate.unwrap_or("?");
                        let to = info.name();
                        write!(f, "<div class=\"crate-boundary\">─── ")?;
                        write!(f, "<span class=\"crate-name\">")?;
                        write_html_escaped(f, from)?;
                        write!(f, "</span> (above) → <span class=\"crate-name\">")?;
                        write_html_escaped(f, to)?;
                        writeln!(f, "</span> (below) ───</div>")?;
                        current_crate = Some(to);
                        link_template = build_link_base(info);
                    }
                }

                match loc_opt {
                    Some(loc) => {
                        write!(f, "<div class=\"location\">")?;
                        write!(f, "<span class=\"at-prefix\">at </span>")?;

                        if let Some(ref template) = link_template {
                            let url = template
                                .replace("{file}", loc.file())
                                .replace("{line}", &loc.line().to_string());
                            write!(f, "<a href=\"")?;
                            write_html_escaped(f, &url)?;
                            write!(f, "\" target=\"_blank\">")?;
                            write!(f, "<span class=\"file\">")?;
                            write_html_escaped(f, loc.file())?;
                            write!(f, "</span>")?;
                            write!(f, "<span class=\"at-prefix\">:</span>")?;
                            write!(f, "<span class=\"line\">{}</span>", loc.line())?;
                            write!(f, "</a>")?;
                        } else {
                            write!(f, "<span class=\"file\">")?;
                            write_html_escaped(f, loc.file())?;
                            write!(f, "</span>")?;
                            write!(f, "<span class=\"at-prefix\">:</span>")?;
                            write!(f, "<span class=\"line\">{}</span>", loc.line())?;
                        }
                        writeln!(f, "</div>")?;

                        // Contexts (skip crate boundaries, already shown)
                        for context in trace.contexts_at(i) {
                            if matches!(context, AtContext::Crate(_)) {
                                continue;
                            }
                            write!(f, "<div class=\"context\">╰─ ")?;
                            match context {
                                AtContext::Text(msg) => {
                                    write!(f, "<span class=\"context-text\">")?;
                                    write_html_escaped(f, msg.as_ref())?;
                                    writeln!(f, "</span></div>")?;
                                }
                                AtContext::FunctionName(name) => {
                                    write!(f, "in <span class=\"context-fn\">")?;
                                    write_html_escaped(f, name)?;
                                    writeln!(f, "</span></div>")?;
                                }
                                AtContext::Debug(t) => {
                                    write!(f, "<span class=\"context-data\">")?;
                                    write_html_escaped(f, &format!("{:?}", t))?;
                                    writeln!(f, "</span></div>")?;
                                }
                                AtContext::Display(t) => {
                                    write!(f, "<span class=\"context-data\">")?;
                                    write_html_escaped(f, &format!("{}", t))?;
                                    writeln!(f, "</span></div>")?;
                                }
                                AtContext::Error(e) => {
                                    write!(f, "caused by: <span class=\"context-error\">")?;
                                    write_html_escaped(f, &format!("{}", e))?;
                                    writeln!(f, "</span></div>")?;
                                }
                                AtContext::Crate(_) => unreachable!(),
                            }
                        }
                    }
                    None => {
                        writeln!(f, "<div class=\"skip-marker\">[...]</div>")?;
                    }
                }
            }

            writeln!(f, "</div>")?;
            Ok(())
        }
    }

    fn write_html_escaped(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
        for c in s.chars() {
            match c {
                '<' => f.write_str("&lt;")?,
                '>' => f.write_str("&gt;")?,
                '&' => f.write_str("&amp;")?,
                '"' => f.write_str("&quot;")?,
                '\'' => f.write_str("&#39;")?,
                _ => f.write_char(c)?,
            }
        }
        Ok(())
    }

    fn build_link_base(info: &AtCrateInfo) -> Option<String> {
        let repo = info.repo()?;
        let commit = info.commit()?;
        let path = info.crate_path().unwrap_or("");

        let link_format = info.link_format();

        let base = link_format
            .replace("{repo}", repo)
            .replace("{commit}", commit)
            .replace("{path}", path);

        Some(base)
    }
}

#[cfg(feature = "_html")]
#[allow(unused_imports)]
pub use html::HTML_STYLES;
#[cfg(feature = "_html")]
pub use html::HtmlDisplay;

// ============================================================================
// Extension methods on At<E>
// ============================================================================

impl<E: fmt::Debug> At<E> {
    /// Format the error with terminal colors.
    ///
    /// Requires the `_termcolor` feature.
    #[inline]
    #[cfg(feature = "_termcolor")]
    pub fn display_color(&self) -> TermColorDisplay<'_, E> {
        TermColorDisplay { traced: self }
    }

    /// Format the error with terminal colors and metadata links.
    ///
    /// Requires the `_termcolor` feature.
    #[inline]
    #[cfg(feature = "_termcolor")]
    pub fn display_color_meta(&self) -> TermColorMetaDisplay<'_, E> {
        TermColorMetaDisplay { traced: self }
    }

    /// Format the error as HTML.
    ///
    /// Requires the `_html` feature.
    #[inline]
    #[cfg(feature = "_html")]
    pub fn display_html(&self) -> HtmlDisplay<'_, E> {
        HtmlDisplay {
            traced: self,
            include_styles: false,
        }
    }

    /// Format the error as HTML with embedded CSS styles.
    ///
    /// Requires the `_html` feature.
    #[inline]
    #[cfg(feature = "_html")]
    pub fn display_html_styled(&self) -> HtmlDisplay<'_, E> {
        HtmlDisplay {
            traced: self,
            include_styles: true,
        }
    }
}
