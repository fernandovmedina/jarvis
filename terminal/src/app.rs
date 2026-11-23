//! Dashboard state: ring buffers per panel, focus and scroll.

use std::collections::VecDeque;
use std::path::Path;

use ratatui::crossterm::event::KeyCode;

use crate::parse::{self, AgentEvent, LogLine};
use crate::services;
use crate::sys::Sys;
use crate::tail::Tail;
use crate::telegram::{Telegram, TgMsg};
use crate::util;

/// Max entries kept per panel; memory stays flat however long it runs.
pub const CAP: usize = 500;

#[derive(Clone, Copy, PartialEq)]
pub enum Focus {
    Api = 0,
    Telegram = 1,
    Logs = 2,
}

pub struct ApiEntry {
    pub date: String,
    pub time: String,
    pub ev: AgentEvent,
}

pub struct LogEntry {
    pub src: &'static str,
    /// None for continuation lines (tracebacks, multi-line messages).
    pub line: Option<LogLine>,
    pub raw: String,
}

#[derive(Default)]
pub struct Stats {
    pub date: String,
    pub calls: u64,
    pub input: u64,
    pub output: u64,
    pub tool_calls: u64,
    pub tool_errors: u64,
}

pub struct App {
    pub sys: Sys,
    pub services: Vec<(&'static str, String)>,
    pub api: VecDeque<ApiEntry>,
    pub stats: Stats,
    pub tg: VecDeque<TgMsg>,
    pub tg_err: Option<String>,
    pub logs: VecDeque<LogEntry>,
    pub focus: Focus,
    /// Lines scrolled up from the bottom, per Focus; 0 = following newest.
    pub scroll: [usize; 3],
    pub quit: bool,
    telegram: Telegram,
    agent_tail: Tail,
    gw_tail: Tail,
    err_tail: Tail,
}

fn push<T>(q: &mut VecDeque<T>, v: T) {
    if q.len() == CAP {
        q.pop_front();
    }
    q.push_back(v);
}

fn log_entry(src: &'static str, raw: String) -> LogEntry {
    let raw = util::sanitize(&raw);
    LogEntry { src, line: parse::parse_line(&raw), raw }
}

impl App {
    pub fn new(hermes: &Path) -> Self {
        let logs = hermes.join("logs");
        let mut app = App {
            sys: Sys::default(),
            services: Vec::new(),
            api: VecDeque::new(),
            stats: Stats { date: util::today(), ..Default::default() },
            tg: VecDeque::new(),
            tg_err: None,
            logs: VecDeque::new(),
            focus: Focus::Telegram,
            scroll: [0; 3],
            quit: false,
            telegram: Telegram::new(hermes.join("state.db")),
            // agent.log: read a large backlog once so "today" totals are right at start-up
            agent_tail: Tail::new(logs.join("agent.log"), 8 << 20),
            gw_tail: Tail::new(logs.join("gateway.log"), 128 << 10),
            err_tail: Tail::new(logs.join("errors.log"), 128 << 10),
        };
        app.load_log_backlog();
        app
    }

    /// First read of gateway.log + errors.log, merged in time order.
    fn load_log_backlog(&mut self) {
        let mut groups: Vec<(String, Vec<LogEntry>)> = Vec::new();
        for (src, lines) in [("gw", self.gw_tail.poll()), ("err", self.err_tail.poll())] {
            for raw in lines {
                let e = log_entry(src, raw);
                match &e.line {
                    Some(l) => groups.push((format!("{} {}", l.date, l.time), vec![e])),
                    None => match groups.last_mut() {
                        Some(g) if g.1[0].src == src => g.1.push(e),
                        _ => groups.push((String::new(), vec![e])),
                    },
                }
            }
        }
        groups.sort_by(|a, b| a.0.cmp(&b.0)); // stable: same-second lines keep file order
        for (_, g) in groups {
            for e in g {
                push(&mut self.logs, e);
            }
        }
    }

    pub fn tick_fast(&mut self) {
        self.sys.fast();
        let today = util::today();
        if self.stats.date != today {
            self.stats = Stats { date: today, ..Default::default() };
        }
    }

    pub fn tick_slow(&mut self) {
        self.sys.slow();
        self.services = services::check();
    }

    /// Returns true if anything new arrived.
    pub fn poll_logs(&mut self) -> bool {
        let mut changed = false;
        let today = util::today();

        let mut n_api = 0;
        for raw in self.agent_tail.poll() {
            let Some(l) = parse::parse_line(&raw) else { continue };
            let Some(ev) = parse::parse_agent(&l) else { continue };
            if l.date == today {
                if self.stats.date != today {
                    self.stats = Stats { date: today.clone(), ..Default::default() };
                }
                match &ev {
                    AgentEvent::Api { input, output, .. } => {
                        self.stats.calls += 1;
                        self.stats.input += input;
                        self.stats.output += output;
                    }
                    AgentEvent::Tool { ok, .. } => {
                        self.stats.tool_calls += 1;
                        if !ok {
                            self.stats.tool_errors += 1;
                        }
                    }
                }
            }
            push(&mut self.api, ApiEntry { date: l.date, time: l.time, ev });
            n_api += 1;
        }
        if n_api > 0 {
            changed = true;
            self.keep_view(Focus::Api, n_api);
        }

        let mut n_logs = 0;
        for (src, lines) in [("gw", self.gw_tail.poll()), ("err", self.err_tail.poll())] {
            for raw in lines {
                push(&mut self.logs, log_entry(src, raw));
                n_logs += 1;
            }
        }
        if n_logs > 0 {
            changed = true;
            self.keep_view(Focus::Logs, n_logs);
        }
        changed
    }

    pub fn poll_telegram(&mut self) -> bool {
        match self.telegram.poll() {
            Ok(msgs) => {
                let had_err = self.tg_err.take().is_some();
                let n = msgs.len();
                for mut m in msgs {
                    m.text = util::sanitize(&m.text);
                    push(&mut self.tg, m);
                }
                if n > 0 {
                    self.keep_view(Focus::Telegram, n);
                }
                n > 0 || had_err
            }
            Err(e) => {
                let changed = self.tg_err.as_deref() != Some(e.as_str());
                self.tg_err = Some(e);
                changed
            }
        }
    }

    /// When the user is scrolled up, new entries should not move what they are reading.
    fn keep_view(&mut self, f: Focus, added: usize) {
        let s = &mut self.scroll[f as usize];
        if *s > 0 {
            *s = s.saturating_add(added);
        }
    }

    pub fn key(&mut self, code: KeyCode) {
        let i = self.focus as usize;
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Api => Focus::Telegram,
                    Focus::Telegram => Focus::Logs,
                    Focus::Logs => Focus::Api,
                }
            }
            KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Api => Focus::Logs,
                    Focus::Telegram => Focus::Api,
                    Focus::Logs => Focus::Telegram,
                }
            }
            KeyCode::Char('1') => self.focus = Focus::Api,
            KeyCode::Char('2') => self.focus = Focus::Telegram,
            KeyCode::Char('3') => self.focus = Focus::Logs,
            KeyCode::Up | KeyCode::Char('k') => self.scroll[i] = self.scroll[i].saturating_add(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll[i] = self.scroll[i].saturating_sub(1),
            KeyCode::PageUp => self.scroll[i] = self.scroll[i].saturating_add(10),
            KeyCode::PageDown => self.scroll[i] = self.scroll[i].saturating_sub(10),
            KeyCode::Home | KeyCode::Char('g') => self.scroll[i] = usize::MAX, // clamped when drawn
            KeyCode::End | KeyCode::Char('G') => self.scroll[i] = 0,
            _ => {}
        }
    }
}
