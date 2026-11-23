//! Parsers for Hermes log lines.
//!
//! Format: `2026-11-23 12:37:57,317 INFO [session] module.name: message`
//! (the `[session]` part is optional).

#[derive(Clone, Debug, PartialEq)]
pub struct LogLine {
    pub date: String,
    pub time: String,
    pub level: String,
    pub module: String,
    pub msg: String,
}

impl LogLine {
    pub fn is_problem(&self) -> bool {
        matches!(self.level.as_str(), "WARNING" | "ERROR" | "CRITICAL")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    Api { model: String, provider: String, input: u64, output: u64, latency: String, cache: String },
    Tool { name: String, ok: bool, dur: String },
}

pub fn parse_line(s: &str) -> Option<LogLine> {
    let b = s.as_bytes();
    if b.len() < 24 || b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' {
        return None;
    }
    let date = s.get(0..10)?;
    let time = s.get(11..19)?;
    let rest = s
        .get(19..)?
        .trim_start_matches(|c: char| c == ',' || c == '.' || c.is_ascii_digit())
        .trim_start();
    let (level, rest) = rest.split_once(' ')?;
    let rest = rest.trim_start();
    let rest = if rest.starts_with('[') {
        rest.split_once("] ").map(|x| x.1).unwrap_or(rest)
    } else {
        rest
    };
    let (module, msg) = match rest.split_once(": ") {
        Some((m, r)) if !m.is_empty() && !m.contains(' ') => (m, r),
        _ => ("", rest),
    };
    Some(LogLine {
        date: date.into(),
        time: time.into(),
        level: level.into(),
        module: module.into(),
        msg: msg.into(),
    })
}

/// API call / tool events from agent.log.
pub fn parse_agent(l: &LogLine) -> Option<AgentEvent> {
    let m = l.msg.as_str();
    if let Some(i) = m.find("API call #") {
        let (mut model, mut provider, mut latency, mut cache) = (String::new(), String::new(), String::new(), String::new());
        let (mut input, mut output) = (0, 0);
        for tok in m[i..].split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                match k {
                    "model" => model = v.into(),
                    "provider" => provider = v.into(),
                    "in" => input = v.parse().unwrap_or(0),
                    "out" => output = v.parse().unwrap_or(0),
                    "latency" => latency = v.into(),
                    _ => {}
                }
            } else if tok.starts_with('(') && tok.ends_with("%)") {
                cache = tok[1..tok.len() - 1].into();
            }
        }
        return Some(AgentEvent::Api { model, provider, input, output, latency, cache });
    }

    if l.module.ends_with("tool_executor") {
        // "tool memory completed (0.03s, 1045 chars)"
        // "Tool memory returned error (0.01s): {...}"
        let rest = m.strip_prefix("tool ").or_else(|| m.strip_prefix("Tool "))?;
        let (name, after) = rest.split_once(' ')?;
        let ok = if after.starts_with("completed") {
            true
        } else if after.contains("error") || after.contains("failed") {
            false
        } else {
            return None;
        };
        let dur = after
            .split_once('(')
            .and_then(|(_, r)| r.split([',', ')', ':']).next())
            .unwrap_or("")
            .to_string();
        return Some(AgentEvent::Tool { name: name.into(), ok, dur });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_call() {
        let s = "2026-11-23 12:37:57,317 INFO [20261122_212913_f3ab038a] agent.conversation_loop: API call #30: model=deepseek-v4-flash provider=deepseek in=118350 out=777 total=119127 latency=4.9s cache=118144/118350 (100%) id=1cfcc60d-";
        let l = parse_line(s).unwrap();
        assert_eq!(l.time, "12:37:57");
        assert_eq!(l.level, "INFO");
        assert_eq!(l.module, "agent.conversation_loop");
        assert_eq!(
            parse_agent(&l),
            Some(AgentEvent::Api {
                model: "deepseek-v4-flash".into(),
                provider: "deepseek".into(),
                input: 118350,
                output: 777,
                latency: "4.9s".into(),
                cache: "100%".into(),
            })
        );
    }

    #[test]
    fn tool_ok_and_error() {
        let ok = parse_line("2026-11-23 12:38:01,001 INFO [x] agent.tool_executor: tool memory completed (0.03s, 1045 chars)").unwrap();
        assert_eq!(parse_agent(&ok), Some(AgentEvent::Tool { name: "memory".into(), ok: true, dur: "0.03s".into() }));
        let err = parse_line("2026-11-23 12:37:57,373 WARNING [x] agent.tool_executor: Tool memory returned error (0.01s): {\"success\": false}").unwrap();
        assert!(err.is_problem());
        assert_eq!(parse_agent(&err), Some(AgentEvent::Tool { name: "memory".into(), ok: false, dur: "0.01s".into() }));
    }

    #[test]
    fn gateway_line() {
        let l = parse_line("2026-11-23 12:38:03,532 INFO gateway.platforms.base: [Telegram] Sending response (444 chars) to 8101322514").unwrap();
        assert_eq!(l.module, "gateway.platforms.base");
        assert_eq!(l.msg, "[Telegram] Sending response (444 chars) to 8101322514");
        assert_eq!(parse_agent(&l), None);
    }

    #[test]
    fn not_a_log_line() {
        assert_eq!(parse_line("Traceback (most recent call last):"), None);
        assert_eq!(parse_line("  File \"x.py\", line 1"), None);
    }
}
