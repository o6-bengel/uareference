use std::io::Write;
use std::process::{
    Command,
    Stdio,
};

use base64::Engine;

/// Copies through the terminal (OSC 52), which works over SSH and inside tmux with `set-clipboard on`.
pub fn copy(text: &str) -> std::io::Result<()> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = std::io::stdout();
    write!(out, "\x1b]52;c;{encoded}\x07")?;
    out.flush()
}

pub fn open_url(url: &str) -> std::io::Result<()> {
    if is_wsl() {
        for (program, args) in [("wslview", vec![url]), ("explorer.exe", vec![url])] {
            if spawn(program, &args).is_ok() {
                return Ok(());
            }
        }
    }
    open::that_detached(url)
}

fn spawn(
    program: &str,
    args: &[&str],
) -> std::io::Result<()> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/sys/kernel/osrelease").is_ok_and(|r| r.to_lowercase().contains("microsoft"))
}
