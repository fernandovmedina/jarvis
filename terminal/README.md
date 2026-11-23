# jarvis-term

Full-screen text dashboard for the Jarvis server's console. It shows Telegram traffic, the model's API calls, hardware state, addresses and logs. It uses green and white on black, ASCII only, no icons or graphics.

```
 JARVIS  |  jarvis-agent  |  wlp1s0 192.168.0.84  |  up 20:43  |  load 0.85 0.73 0.40  |  2026-11-23 12:53:04
+ SYSTEM ------------------------------++ API CALLS (agent.log) -------------------------------------------+
|CPU  [#.....................]   2%    ||today 374 calls  in 64.3M  out 302.6k  tools 402  tool errors 22  |
|     c0   2%  c1   6%  c2   1%  c3  2%||12:37:57    API  deepseek-v4-flash (deepseek)  in 118.3k  out 777 |
|RAM  [##########............] 1.6G/3.7G|12:37:57    TOOL memory  ERROR  0.01s                           |
|TEMP pkg 44C  c0 43C  c1 44C          ||12:38:01    TOOL memory  ok  0.03s                              |
|SVC  gateway up  tunnel up  sshd up   ||12:38:03    API  deepseek-v4-flash (deepseek)  in 120.1k  out 153 |
+ TELEGRAM ---------------------------------------------------------------------------------------------+
|12:37:51    Fer     no, lo de las imagenes lo dejamos para despues                                        |
|12:38:03    Jarvis  Entendido, imágenes en pausa.                                                         |
+ LOGS (gateway.log + errors.log) ----------------------------------------------------------------------+
|12:38:03    gw  INFO gateway.platforms.base: [Telegram] Sending response (444 chars) to ...              |
+-------------------------------------------------------------------------------------------------------+
```

## Panels

| Panel | Shows | Source | Refresh |
|---|---|---|---|
| Header | hostname, IPv4 per interface, uptime, load, clock | `/proc/sys/kernel/hostname`, `getifaddrs`, `/proc/uptime`, `/proc/loadavg` | 1 s / 10 s |
| SYSTEM | CPU total + per core, RAM, swap, temperatures, disk `/`, network rate, battery/AC, services | `/proc/stat`, `/proc/meminfo`, hwmon `coretemp`, `statvfs`, `/proc/net/dev`, `/sys/class/power_supply`, `systemctl is-active` | 1 s (disk, battery, services: 10 s) |
| API CALLS | each model call (model, provider, tokens in/out, latency, cache hit) and each tool call (ok / ERROR, duration); today's totals | `~/.hermes/logs/agent.log` | 0.5 s |
| TELEGRAM | owner (`Fer`) and `Jarvis` messages, wrapped | `~/.hermes/state.db`, opened read-only, sessions with `source = 'telegram'` | 2 s |
| LOGS | gateway activity and every warning/error | `~/.hermes/logs/gateway.log`, `~/.hermes/logs/errors.log` | 0.5 s |

The `tunnel`, `sshd` and `gateway` services map to `cloudflared.service`, `sshd.service` and the user unit `hermes-gateway.service`. Anything other than `active` shows in bold.

Bold white marks warnings, errors, tool failures, temperatures at 80 °C or above, RAM at 90 % or above, and a stopped service.

## Keys

| Key | Action |
|---|---|
| `q`, `Esc`, `Ctrl+C` | quit |
| `Tab`, `Shift+Tab`, `1` `2` `3` | focus API CALLS / TELEGRAM / LOGS (the focused title is bold) |
| `Up`/`Down` (`k`/`j`), `PgUp`/`PgDn` | scroll the focused panel |
| `Home` (`g`) / `End` (`G`) | oldest / follow newest |

When a panel is scrolled up, new lines do not move the view, and the title shows `[up N, End to follow]`.

## Cost

Single thread, no async runtime, no GC. On the Surface Pro 3 (i5-4300U) it measured about 9 MB RSS and roughly 1 % CPU on one core. The 2.7 MB stripped binary has SQLite compiled in, so it needs no `sqlite-devel`. Each panel keeps at most 500 entries.

## Build and install

It runs as the user that owns `~/.hermes` (`~/.hermes/logs` is mode 700), and needs Rust ≥ 1.80.

```bash
cd terminal
cargo test --release        # log parser tests
cargo build --release       # ~2 min first time on the i5 (bundled SQLite + LTO)
sudo install -m 0755 target/release/jarvis-term /usr/bin/jarvis-term
```

Run it from the console (log in on tty1) or over SSH:

```bash
jarvis-term
```

The dashboard fills whatever terminal it runs in and redraws on resize. On the physical console a larger font helps: `sudo setfont ter-v22n` (session only).

`HERMES_HOME` overrides the Hermes directory (default `$HOME/.hermes`).

## Files

```
src/main.rs       terminal setup/restore, event loop and timers
src/app.rs        state, ring buffers, keys, scroll
src/ui.rs         layout and rendering (two colors, ASCII borders/bars)
src/sys.rs        /proc and /sys readers
src/services.rs   systemctl is-active
src/telegram.rs   read-only SQLite polling of state.db
src/tail.rs       log follower (handles truncation and rotation)
src/parse.rs      Hermes log line parsers + tests
src/util.rs       local time, text sanitizing for the console font, formatting
```
