//! Service health via `systemctl is-active` (one short process call every 10 s).

use std::process::{Command, Stdio};

fn is_active(args: &[&str]) -> Vec<String> {
    Command::new("systemctl")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_string()).collect())
        .unwrap_or_default()
}

/// (label, state) for the units Jarvis depends on.
pub fn check() -> Vec<(&'static str, String)> {
    let user = is_active(&["--user", "is-active", "hermes-gateway.service"]);
    let system = is_active(&["is-active", "cloudflared.service", "sshd.service"]);
    let get = |v: &Vec<String>, i: usize| v.get(i).cloned().unwrap_or_else(|| "?".into());
    vec![
        ("gateway", get(&user, 0)),
        ("tunnel", get(&system, 0)),
        ("sshd", get(&system, 1)),
    ]
}
