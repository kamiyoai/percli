use owo_colors::OwoColorize;
use std::time::Instant;

/// Print a cargo-style status line on stderr.
/// The verb is right-justified to 12 chars, bold and green.
pub fn status(verb: &str, message: &str) {
    let styled = format!("{:>12}", verb);
    eprintln!(
        "{} {}",
        styled.if_supports_color(owo_colors::Stream::Stderr, |t| t.green().to_string()),
        message
    );
}

/// Cyan status line on stderr.
pub fn status_cyan(verb: &str, message: &str) {
    let styled = format!("{:>12}", verb);
    eprintln!(
        "{} {}",
        styled.if_supports_color(owo_colors::Stream::Stderr, |t| t.cyan().to_string()),
        message
    );
}

/// Yellow warning status line on stderr.
#[allow(dead_code)]
pub fn status_warn(verb: &str, message: &str) {
    let styled = format!("{:>12}", verb);
    eprintln!(
        "{} {}",
        styled.if_supports_color(owo_colors::Stream::Stderr, |t| t.yellow().to_string()),
        message
    );
}

/// Print "Finished in X.XXs" on stderr.
pub fn finished(start: Instant) {
    let elapsed = start.elapsed();
    let styled = format!("{:>12}", "Finished");
    eprintln!(
        "{} in {:.2}s",
        styled.if_supports_color(owo_colors::Stream::Stderr, |t| t.green().to_string()),
        elapsed.as_secs_f64()
    );
}

/// Print a structured error on stderr: "error: msg"
pub fn error(msg: &str) {
    eprintln!(
        "{}: {}",
        "error".if_supports_color(owo_colors::Stream::Stderr, |t| t.red().to_string()),
        msg
    );
}

/// Print a structured error with optional path and help hint.
pub fn error_with_help(msg: &str, path: Option<&str>, help: Option<&str>) {
    error(msg);
    if let Some(p) = path {
        eprintln!(
            "  {} {}",
            "-->".if_supports_color(owo_colors::Stream::Stderr, |t| t.cyan().to_string()),
            p
        );
    }
    if let Some(h) = help {
        eprintln!(
            " {}: {}",
            "help".if_supports_color(owo_colors::Stream::Stderr, |t| t.cyan().to_string()),
            h
        );
    }
}
