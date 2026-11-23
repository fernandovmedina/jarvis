//! Layout and rendering. Two colors only (green, white), ASCII borders and bars,
//! so it reads well on the Linux console with any font.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::parse::AgentEvent;
use crate::util;

const ASCII: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

fn g() -> Style {
    Style::default().fg(Color::Green)
}
fn w() -> Style {
    Style::default().fg(Color::White)
}
fn b() -> Style {
    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
}

fn panel(title: &str, focused: bool, scroll: usize) -> Block<'static> {
    let mut t = format!(" {title} ");
    if scroll > 0 {
        t.push_str(&format!("[up {scroll}, End to follow] "));
    }
    Block::bordered()
        .border_set(ASCII)
        .border_style(g())
        .title(Span::styled(t, if focused { b() } else { g() }))
}

fn bar(pct: f64, width: usize) -> String {
    let n = ((pct.clamp(0.0, 100.0) / 100.0) * width as f64).round() as usize;
    format!("[{}{}]", "#".repeat(n), ".".repeat(width - n.min(width)))
}

fn kv(label: &str) -> Span<'static> {
    Span::styled(format!("{label:<5}"), g())
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let [head, top, mid, low, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(10),
        Constraint::Fill(3),
        Constraint::Fill(2),
        Constraint::Length(1),
    ])
    .areas(f.area());
    let [sys_a, api_a] =
        Layout::horizontal([Constraint::Percentage(42), Constraint::Fill(1)]).areas(top);

    header(f, head, app);
    system(f, sys_a, app);
    api(f, api_a, app);
    telegram(f, mid, app);
    logs(f, low, app);

    f.render_widget(
        Paragraph::new(Line::styled(
            " q quit | Tab or 1 2 3 focus | Up/Down PgUp/PgDn scroll | Home oldest | End follow",
            g(),
        )),
        foot,
    );
}

fn header(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.sys;
    let sep = || Span::styled("  |  ", g());
    let mut spans = vec![
        Span::styled(" JARVIS ", b()),
        sep(),
        Span::styled(s.hostname.clone(), w()),
    ];
    for (name, ip) in s.ips.iter().take(2) {
        spans.push(sep());
        spans.push(Span::styled(format!("{name} "), g()));
        spans.push(Span::styled(ip.to_string(), b()));
    }
    if s.ips.is_empty() {
        spans.push(sep());
        spans.push(Span::styled("NO IP", b()));
    }
    let now = util::local(util::now_epoch() as i64);
    spans.extend([
        sep(),
        Span::styled("up ", g()),
        Span::styled(util::uptime(s.uptime), w()),
        sep(),
        Span::styled("load ", g()),
        Span::styled(format!("{:.2} {:.2} {:.2}", s.load[0], s.load[1], s.load[2]), w()),
        sep(),
        Span::styled(
            format!(
                "{} {:02}:{:02}:{:02}",
                util::date_of(&now),
                now.hour,
                now.min,
                now.sec
            ),
            w(),
        ),
    ]);
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn system(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.sys;
    let inner_w = area.width.saturating_sub(2) as usize;
    let bw = inner_w.saturating_sub(20).clamp(5, 40);
    let mut lines = Vec::new();

    let cpu = s.cpu.first().copied().unwrap_or(0.0);
    lines.push(Line::from(vec![
        kv("CPU"),
        Span::styled(bar(cpu, bw), w()),
        Span::styled(format!(" {cpu:>3.0}%"), if cpu >= 90.0 { b() } else { w() }),
    ]));
    let mut cores = vec![Span::raw("     ")];
    for (i, c) in s.cpu.iter().skip(1).enumerate() {
        cores.push(Span::styled(format!("c{i} "), g()));
        cores.push(Span::styled(format!("{c:>3.0}%  "), w()));
    }
    lines.push(Line::from(cores));

    let used = s.mem_total.saturating_sub(s.mem_avail);
    let mp = if s.mem_total > 0 { 100.0 * used as f64 / s.mem_total as f64 } else { 0.0 };
    lines.push(Line::from(vec![
        kv("RAM"),
        Span::styled(bar(mp, bw), w()),
        Span::styled(
            format!(" {}/{}", util::kib(used), util::kib(s.mem_total)),
            if mp >= 90.0 { b() } else { w() },
        ),
    ]));
    let su = s.swap_total.saturating_sub(s.swap_free);
    let sp = if s.swap_total > 0 { 100.0 * su as f64 / s.swap_total as f64 } else { 0.0 };
    lines.push(Line::from(vec![
        kv("SWAP"),
        Span::styled(bar(sp, bw), w()),
        Span::styled(format!(" {}/{}", util::kib(su), util::kib(s.swap_total)), w()),
    ]));

    let mut temp = vec![kv("TEMP")];
    if s.temps.is_empty() {
        temp.push(Span::styled("n/a", w()));
    }
    for (name, t) in &s.temps {
        temp.push(Span::styled(format!("{name} "), g()));
        temp.push(Span::styled(format!("{t:.0}C  "), if *t >= 80.0 { b() } else { w() }));
    }
    lines.push(Line::from(temp));

    lines.push(Line::from(vec![
        kv("DISK"),
        Span::styled(format!("/ {}/{}", util::bytes(s.disk_used), util::bytes(s.disk_total)), w()),
        Span::styled("   NET ", g()),
        Span::styled(format!("rx {} tx {}", util::rate(s.rx_rate), util::rate(s.tx_rate)), w()),
    ]));

    let mut pw = vec![kv("BAT")];
    match &s.battery {
        Some((c, st)) => pw.push(Span::styled(format!("{c}% {st}"), w())),
        None => pw.push(Span::styled("n/a", w())),
    }
    if let Some(ac) = s.ac {
        pw.push(Span::styled("   AC ", g()));
        pw.push(Span::styled(if ac { "online" } else { "OFFLINE" }, if ac { w() } else { b() }));
    }
    lines.push(Line::from(pw));

    let mut svc = vec![kv("SVC")];
    for (name, state) in &app.services {
        let up = state == "active";
        svc.push(Span::styled(format!("{name} "), g()));
        svc.push(Span::styled(
            format!("{}  ", if up { "up" } else { state.as_str() }),
            if up { w() } else { b() },
        ));
    }
    lines.push(Line::from(svc));

    f.render_widget(Paragraph::new(lines).block(panel("SYSTEM", false, 0)), area);
}

/// Wraps `text` after a prefix; continuation lines are indented to the prefix width.
fn wrap(prefix: Vec<Span<'static>>, text: &str, style: Style, width: usize, out: &mut Vec<Line<'static>>) {
    let pw: usize = prefix.iter().map(|s| s.content.chars().count()).sum();
    let avail = width.saturating_sub(pw).max(10);
    let mut first = true;
    let mut emit = |seg: String, out: &mut Vec<Line<'static>>| {
        let mut spans = if first { prefix.clone() } else { vec![Span::raw(" ".repeat(pw))] };
        first = false;
        spans.push(Span::styled(seg, style));
        out.push(Line::from(spans));
    };
    let paras: Vec<&str> = text.split('\n').filter(|p| !p.trim().is_empty()).collect();
    if paras.is_empty() {
        emit(String::new(), out);
        return;
    }
    for para in paras {
        let chars: Vec<char> = para.trim_end().chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let end = (i + avail).min(chars.len());
            let mut cut = end;
            if end < chars.len() {
                if let Some(p) = chars[i..end].iter().rposition(|&c| c == ' ') {
                    if p > avail / 2 {
                        cut = i + p + 1;
                    }
                }
            }
            emit(chars[i..cut].iter().collect(), out);
            i = cut;
        }
    }
}

/// Renders the last lines that fit, honoring (and clamping) the scroll offset.
fn scrolled(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>, scroll: &mut usize) {
    let h = area.height as usize;
    let max = lines.len().saturating_sub(h);
    if *scroll > max {
        *scroll = max;
    }
    let end = lines.len() - *scroll;
    let start = end.saturating_sub(h);
    f.render_widget(Paragraph::new(lines[start..end].to_vec()), area);
}

fn api(f: &mut Frame, area: Rect, app: &mut App) {
    let i = Focus::Api as usize;
    let block = panel("API CALLS (agent.log)", app.focus == Focus::Api, app.scroll[i]);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [sum_a, body] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);

    let st = &app.stats;
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("today ", g()),
            Span::styled(format!("{} calls", st.calls), w()),
            Span::styled("  in ", g()),
            Span::styled(util::tokens(st.input), w()),
            Span::styled("  out ", g()),
            Span::styled(util::tokens(st.output), w()),
            Span::styled("  tools ", g()),
            Span::styled(st.tool_calls.to_string(), w()),
            Span::styled("  tool errors ", g()),
            Span::styled(st.tool_errors.to_string(), if st.tool_errors > 0 { b() } else { w() }),
        ])),
        sum_a,
    );

    let today = util::today();
    let width = body.width as usize;
    let mut lines = Vec::new();
    for e in &app.api {
        let ts = Span::styled(format!("{} ", util::stamp(&e.date, &e.time, &today)), g());
        let (tag, text, style) = match &e.ev {
            AgentEvent::Api { model, provider, input, output, latency, cache } => (
                "API  ",
                format!(
                    "{model} ({provider})  in {}  out {}  {latency}  cache {}",
                    util::tokens(*input),
                    util::tokens(*output),
                    if cache.is_empty() { "-" } else { cache }
                ),
                w(),
            ),
            AgentEvent::Tool { name, ok: true, dur } => ("TOOL ", format!("{name}  ok  {dur}"), w()),
            AgentEvent::Tool { name, ok: false, dur } => ("TOOL ", format!("{name}  ERROR  {dur}"), b()),
        };
        wrap(vec![ts, Span::styled(tag, g())], &text, style, width, &mut lines);
    }
    if lines.is_empty() {
        lines.push(Line::styled("no API calls in agent.log yet", g()));
    }
    scrolled(f, body, lines, &mut app.scroll[i]);
}

fn telegram(f: &mut Frame, area: Rect, app: &mut App) {
    let i = Focus::Telegram as usize;
    let block = panel("TELEGRAM", app.focus == Focus::Telegram, app.scroll[i]);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let today = util::today();
    let width = inner.width as usize;
    let mut lines = Vec::new();
    if let Some(e) = &app.tg_err {
        lines.push(Line::styled(format!("state.db: {e}"), b()));
    }
    for m in &app.tg {
        let prefix = vec![
            Span::styled(format!("{} ", util::stamp_epoch(m.ts, &today)), g()),
            Span::styled(if m.user { "Fer     " } else { "Jarvis  " }, if m.user { b() } else { g() }),
        ];
        wrap(prefix, &m.text, w(), width, &mut lines);
    }
    if lines.is_empty() {
        lines.push(Line::styled("no Telegram messages yet", g()));
    }
    scrolled(f, inner, lines, &mut app.scroll[i]);
}

fn short_level(level: &str) -> &str {
    match level {
        "WARNING" => "WARN",
        "ERROR" => "ERR",
        "CRITICAL" => "CRIT",
        "DEBUG" => "DBG",
        "INFO" => "INFO",
        _ => "?",
    }
}

fn logs(f: &mut Frame, area: Rect, app: &mut App) {
    let i = Focus::Logs as usize;
    let block = panel("LOGS (gateway.log + errors.log)", app.focus == Focus::Logs, app.scroll[i]);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let today = util::today();
    let width = inner.width as usize;
    let mut lines = Vec::new();
    for e in &app.logs {
        match &e.line {
            Some(l) => {
                let style = if l.is_problem() { b() } else { w() };
                let prefix = vec![
                    Span::styled(format!("{} ", util::stamp(&l.date, &l.time, &today)), g()),
                    Span::styled(format!("{:<4}", e.src), g()),
                    Span::styled(format!("{:<5}", short_level(&l.level)), style),
                ];
                let text = if l.module.is_empty() { l.msg.clone() } else { format!("{}: {}", l.module, l.msg) };
                wrap(prefix, &text, style, width, &mut lines);
            }
            None => {
                let prefix = vec![Span::raw(" ".repeat(12)), Span::styled(format!("{:<4}", e.src), g())];
                wrap(prefix, &e.raw, w(), width, &mut lines);
            }
        }
    }
    if lines.is_empty() {
        lines.push(Line::styled("no log lines yet", g()));
    }
    scrolled(f, inner, lines, &mut app.scroll[i]);
}
