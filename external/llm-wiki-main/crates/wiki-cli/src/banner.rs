//! Startup banner with optional truecolor gradient (TTY + no `NO_COLOR`).
//!
//! Printed **before** `clap` parses args so `wiki-cli --help` still shows the logo.

use std::fmt::Write;
use std::io::{self, IsTerminal};

const RESET: &str = "\x1b[0m";

/// Inner width between `│` borders (monospace columns).
const INNER: usize = 56;

/// Sky → violet → coral, smooth across [0, 1].
fn gradient_rgb(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let (r1, g1, b1) = (56_u8, 189_u8, 248_u8); // sky
    let (r2, g2, b2) = (167_u8, 139_u8, 250_u8); // violet
    let (r3, g3, b3) = (251_u8, 113_u8, 133_u8); // rose
    if t < 0.5 {
        let u = t * 2.0;
        (
            lerp_u8(r1, r2, u),
            lerp_u8(g1, g2, u),
            lerp_u8(b1, b2, u),
        )
    } else {
        let u = (t - 0.5) * 2.0;
        (
            lerp_u8(r2, r3, u),
            lerp_u8(g2, g3, u),
            lerp_u8(b2, b3, u),
        )
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8
}

fn paint_line(line: &str, use_color: bool) -> String {
    if !use_color {
        return line.to_string();
    }
    let n = line.chars().count().max(1);
    let mut out = String::with_capacity(line.len() + n * 20);
    for (i, ch) in line.chars().enumerate() {
        if ch == ' ' {
            out.push(ch);
            continue;
        }
        let t = i as f32 / (n.saturating_sub(1).max(1)) as f32;
        let (r, g, b) = gradient_rgb(t);
        let _ = write!(out, "\x1b[38;2;{};{};{}m{}{}", r, g, b, ch, RESET);
    }
    out
}

fn top_rule() -> String {
    format!("  ╭{}╮", "─".repeat(INNER))
}

fn bottom_rule() -> String {
    format!("  ╰{}╯", "─".repeat(INNER))
}

/// Center `text` inside the inner box; truncates if longer than `INNER`.
fn row(text: &str) -> String {
    let mut s = text.trim().to_string();
    if s.chars().count() > INNER {
        s = s.chars().take(INNER).collect();
    }
    let len = s.chars().count();
    let pad = INNER.saturating_sub(len);
    let left = pad / 2;
    let right = pad - left;
    format!(
        "  │{}{}{}│",
        " ".repeat(left),
        s,
        " ".repeat(right)
    )
}

pub fn print_startup_banner() {
    let use_color = std::env::var_os("NO_COLOR").is_none() && io::stdout().is_terminal();

    let lines = [
        String::new(),
        top_rule(),
        row(""),
        row("rust-llm-wiki"),
        row("LLM Wiki v2 · persistent knowledge kernel"),
        row("Rust · event outbox · RRF · MemPalace"),
        row(""),
        bottom_rule(),
        String::new(),
    ];

    for line in &lines {
        println!("{}", paint_line(line, use_color));
    }
}
