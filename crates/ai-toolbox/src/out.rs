//! Terminal output, matching what the bash prints.
//!
//! Colour goes to a tty and nowhere else, and `NO_COLOR` is honoured - the same rule
//! `bin/ai-toolbox` follows, and the reason its output pipes cleanly into anything.

use std::io::IsTerminal;
use std::sync::OnceLock;

fn coloured() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
}

pub fn dim(text: &str) -> String {
    paint("2", text)
}

pub fn bold(text: &str) -> String {
    paint("1", text)
}

pub fn yellow(text: &str) -> String {
    paint("33", text)
}

pub fn red(text: &str) -> String {
    paint("31", text)
}

fn paint(code: &str, text: &str) -> String {
    if coloured() {
        format!("\u{1b}[{code}m{text}\u{1b}[0m")
    } else {
        text.to_string()
    }
}

/// Commentary goes to stderr so that piping a command's output never picks it up.
pub fn info(message: &str) {
    eprintln!("{}", dim(message));
}

pub fn warn(message: &str) {
    eprintln!("{} {}", yellow("!"), yellow(message));
}

pub fn heading(text: &str) {
    println!("\n{}", bold(text));
}

/// The `  label:      value` column the status output is built from.
pub fn field(label: &str, value: &str) {
    println!("  {:<11}{value}", format!("{label}:"));
}

pub fn none() -> String {
    dim("(none)")
}

/// A space-separated list, or `(none)` - how the bash renders every list it prints.
pub fn list<S: AsRef<str>>(items: &[S]) -> String {
    if items.is_empty() {
        return none();
    }
    items
        .iter()
        .map(|i| i.as_ref())
        .collect::<Vec<_>>()
        .join(" ")
}
