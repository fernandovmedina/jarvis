//! Small helpers: local time without extra crates, text sanitizing, number formatting.

use std::time::{SystemTime, UNIX_EPOCH};

pub struct Tm {
    pub year: i32,
    pub mon: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
}

pub fn local(epoch: i64) -> Tm {
    let t: libc::time_t = epoch as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    Tm {
        year: tm.tm_year + 1900,
        mon: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        min: tm.tm_min as u32,
        sec: tm.tm_sec as u32,
    }
}

pub fn now_epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn date_of(tm: &Tm) -> String {
    format!("{:04}-{:02}-{:02}", tm.year, tm.mon, tm.day)
}

pub fn today() -> String {
    date_of(&local(now_epoch() as i64))
}

/// "HH:MM:SS" for today, "MM-DD HH:MM" for older entries. `date` is YYYY-MM-DD, `time` HH:MM:SS.
pub fn stamp(date: &str, time: &str, today: &str) -> String {
    if date == today || date.len() < 10 || time.len() < 5 {
        format!("{time:<11}")
    } else {
        format!("{} {}", &date[5..10], &time[..5])
    }
}

pub fn stamp_epoch(epoch: f64, today: &str) -> String {
    let tm = local(epoch as i64);
    let time = format!("{:02}:{:02}:{:02}", tm.hour, tm.min, tm.sec);
    stamp(&date_of(&tm), &time, today)
}

/// Keep only characters the Linux console font can draw at width 1.
pub fn sanitize(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\n' => Some('\n'),
            '\t' => Some(' '),
            c if c.is_control() => None,
            c if (c as u32) < 0x250 => Some(c),
            '\u{2018}' | '\u{2019}' => Some('\''),
            '\u{201C}' | '\u{201D}' => Some('"'),
            '\u{2013}' | '\u{2014}' => Some('-'),
            '\u{2026}' => Some('.'),
            '\u{200B}'..='\u{200F}' | '\u{FE0F}' => None,
            _ => Some('?'),
        })
        .collect()
}

/// kB → "1.6G" / "512M".
pub fn kib(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    if mb >= 1024.0 {
        format!("{:.1}G", mb / 1024.0)
    } else {
        format!("{mb:.0}M")
    }
}

pub fn bytes(b: u64) -> String {
    kib(b / 1024)
}

pub fn rate(bps: f64) -> String {
    if bps >= 1024.0 * 1024.0 {
        format!("{:.1}M/s", bps / 1024.0 / 1024.0)
    } else if bps >= 1024.0 {
        format!("{:.0}K/s", bps / 1024.0)
    } else {
        format!("{bps:.0}B/s")
    }
}

pub fn tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

pub fn uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = secs % 86400 / 3600;
    let m = secs % 3600 / 60;
    if d > 0 {
        format!("{d}d {h:02}:{m:02}")
    } else {
        format!("{h:02}:{m:02}")
    }
}
