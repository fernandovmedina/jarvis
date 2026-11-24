# CONTEXT.md — Jarvis

Personal AI assistant ("Jarvis") running 24/7 on a home server, reachable through Telegram, voice and SSH. It manages GitHub repos, Google tools and personal tasks, and keeps a dynamic portfolio up to date.

## Goals

- **Always-on personal assistant** that the owner talks to by text (Telegram) or voice.
- **Dynamic portfolio:** every push to a GitHub repo is reviewed; if it is relevant (employability, tech stack, configuration, technical merit), Jarvis opens a PR on the portfolio adding it. Irrelevant pushes are ignored.
- **Repo awareness:** read personal and Neurovix repos freely for context; any modification goes through a PR and needs the owner's explicit permission.
- **Agenda:** morning briefing with the day's plan; reminders 60, 30 and 5 minutes before each event; notify about agenda changes.
- **Email:** read, delete spam, draft replies; send only after approval.
- **Finances:** track everything (expenses, income, subscriptions) in MXN, in Google Sheets and a local SQLite database.
- **Reminders/alerts:** Telegram text or voice call depending on importance. Jarvis may message at any time and decides when something is important enough to call.
- **Extensible:** more tools will be connected over time. The owner connects tools himself to learn.

## Owner

- Software developer: full-stack, Linux, server configuration, DevOps, networking, infrastructure; learning AI/ML.
- SEO Intern at Justia (automating manual tasks).
- CEO and main developer of **Neurovix** (software company).
- President of the programming club at TecNM Campus Saltillo; hackathons and competitions.
- Location: Monterrey, Nuevo León (UTC-6).
- Jarvis addresses him as: Señor, Sir, Fernando, Fer, MrVix, SrVix, Jefe, Boss.
- Language: replies in the language he writes in (Spanish or English). Tone: formal.

## Hardware

| | |
|---|---|
| Device | Microsoft Surface Pro 3 (model 1631) |
| CPU | Intel Core i5-4300U (2 cores / 4 threads) |
| RAM | 3.7 GB + 3.7 GB swap |
| GPU | Intel HD (Haswell), not usable for AI |
| Disk | 117 GB (LVM `fedora_hermes-root`) |
| Power | Plugged in 24/7; battery kept in, UEFI **Battery Limit** enabled (caps at 50%) |

Too weak to run useful local LLMs; inference is done through cloud APIs.

## Audio

- **Cards:** card 1 `PCH` (Realtek **ALC288**) — laptop speakers and headphone jack, playback `hw:1,0`; card 0 `HDMI` — only `pcmC0D3p/7p/8p`, usable only with a display attached.
- **Stack:** `alsa-utils` 1.2.16 (`aplay`, `amixer`, `speaker-test`, `alsactl`). No PipeWire, no PulseAudio — no daemon, direct ALSA only. The Hermes-bundled `ffmpeg` has only the `pulse` output, so it cannot play to these speakers.
- **Mixer:** `Master Playback Volume` sits after the per-output amps. On a fresh install it was at `0` with the switch off, which silences the speakers while `Speaker` still reads 127. Current state: Master 101 (80 %), Speaker 101, Headphone 101, switches on.
- **Persisted:** `/var/lib/alsa/asound.state`, written by `sudo alsactl store`; `alsa-restore.service` is `WantedBy=sound.target` and restores it at boot, so the level survives reboots.
- **Test:** `speaker-test -D hw:1,0 -c2 -t sine -f 440 -l1` plays 440 Hz on both channels. Confirm the hardware amp, not just the control: `/proc/asound/card1/codec#0` → `Amp-Out vals: [0x4b 0x4b]` (raised); `[0x00 0x00]` means inaudible even when the control reads 127.
- **Write access:** `fernandovmedina` is not in the `audio` group but has an ACL on `/dev/snd/pcmC1D0p` and `/dev/snd/controlC1` — `test -w` is the authoritative check.

## Operating system

- Fedora Server 44, kernel 7.3, headless (TTY only).
- Hostname: `jarvis-agent`.
- Console font: `eurlatgr` (`/etc/vconsole.conf`); a larger Terminus font is preferred.
- Lid close: default behavior (not yet set to ignore).
- Installed: docker 29.10, git, gh 2.97, sqlite3, python3, Node (bundled with Hermes), alsa-utils 1.2.16.

## Network

| | |
|---|---|
| Interface | `wlp1s0` (Wi-Fi) |
| IPv4 | `192.168.0.84/24`, reserved on the router (static lease bound to MAC `8e:2b:14:19:07:1c`) |
| Gateway | `192.168.0.1` |
| IPv6 | Yes (ISP-assigned, SLAAC) |
| Firewall | firewalld active, zone `FedoraServer`: only `ssh` + `dhcpv6-client`, no extra ports or rich rules |
| SSH | sshd active, publickey only (password and keyboard-interactive refused); root login enabled on purpose, key-only |
| Remote access | Cloudflare Tunnel (`cloudflared` 2026.11.1): `jarvis.fervazquezdev.com` → SSH, behind Cloudflare Access |

From the Mac, `ssh jarvis` connects without a password (`~/.ssh/config`: `Host jarvis` → `192.168.0.84`, user `fernandovmedina`).

The server keeps `192.168.0.84` on Wi-Fi: in the router's Private Network table the `jarvis-agent` entry was switched from DHCP to **Static**, reserving that address for its MAC (screenshot: `public/network.png`). The server itself still asks for its address over DHCP; the router always hands back the same one, so `ssh jarvis` keeps working. The MAC is locally administered (NetworkManager's per-network address), so if it ever changes the reservation must be updated. Remote access from outside the home goes through a Cloudflare Tunnel, which also lets GitHub webhooks reach the server for the portfolio workflow without opening router ports.

### Cloudflare Tunnel

- **Client:** `cloudflared` 2026.11.1 (RPM `cloudflared-2026.11.1-1.x86_64`), binary `/usr/bin/cloudflared`; `/usr/local/sbin/cloudflared` is a symlink to it.
- **Mode:** remotely managed and token-based — there is no `config.yml`. The tunnel token lives in `/etc/cloudflared/token` (`root:root`, mode `0600`).
- **Service:** system unit `cloudflared.service` (`/etc/systemd/system/cloudflared.service`), `ExecStart=/usr/bin/cloudflared --no-autoupdate tunnel run --token-file /etc/cloudflared/token`, `Restart=on-failure`, enabled at boot. Logs: `journalctl -u cloudflared -f`.
- **Tunnel ID:** `8d7463f5-dbcc-4874-b1a5-e95c563c6934`.
- **Ingress:** `jarvis.fervazquezdev.com` → `ssh://localhost:22`, catch-all `http_status:404`. Routes are managed in the Cloudflare dashboard (Zero Trust → Networks → Tunnels); the server only holds the token.
- **Protection:** the hostname is behind Cloudflare Access (team domain `delicate-glitter-2023.cloudflareaccess.com`). A plain browser request returns `302` to the Access login, so nothing reaches SSH without an Access session or service token.
- **Connecting from outside:** `ssh -o ProxyCommand="cloudflared access ssh --hostname %h" fernandovmedina@jarvis.fervazquezdev.com` — needs `cloudflared` on the client and a valid Access session/token.
- **Known quirk:** on start-up the journal shows `timeout: no recent network activity` against the IPv6 edge for a couple of connections; cloudflared retries and re-registers over IPv4 (dfw/iah/atl) until all four connections are up. Harmless but noisy.

### Firewall

- **Zone:** `FedoraServer` (default, active) on `wlp1s0` — services `ssh` and `dhcpv6-client` only; no opened ports, no source ports, no rich rules, `masquerade: no`.
- **Forwarding:** the zone says `forward: yes`, but `net.ipv4.ip_forward = 0` and `net.ipv6.conf.all.forwarding = 0`, so it forwards nothing.
- **Exposure:** a single inbound TCP listener on all interfaces, `22` (sshd). Everything else binds loopback: cloudflared metrics `127.0.0.1:20241`, Hermes `127.0.0.1:34701`, systemd-resolved `:53`, chronyd `:323`. The four wildcard UDP sockets are cloudflared's QUIC connections, not services.
- **No WAN path:** traceroute lands on carrier NAT (`187.161.179.5` → `10.254.1.61`), so nothing is reachable from the internet except through the tunnel.
- **Containers:** docker daemon not running, no podman containers — no hidden published ports.
- **Check:** `sudo firewall-cmd --list-all` (as a normal user every rule-reading call fails with `Authorization failed.` — that is polkit, not a broken firewall).

### SSH hardening

- **Where:** drop-in inside `/etc/ssh/sshd_config.d/` (root-only directory) carrying `PasswordAuthentication no` and `KbdInteractiveAuthentication no`; both are in effect. Confirm with `sudo sshd -T | grep -iE 'permitrootlogin|passwordauthentication|kbdinteractive'`.
- **Root login: `PermitRootLogin yes`, by the owner's decision.** `sshd -T` reports `permitrootlogin yes` and the drop-in does not override it. Accepted because root can only present a key (`passwordauthentication no` is global) and the server has no WAN path — the only outside door is the tunnel hostname behind Cloudflare Access, which admits the owner's email. It stops being safe if password or keyboard-interactive auth is re-enabled, a key is added to `/root/.ssh/authorized_keys`, the Access policy is widened, the route loses Access, or the tunnel gains another ingress. Whether `/root/.ssh/authorized_keys` exists is root-only and unchecked. Revert with `echo 'PermitRootLogin no' | sudo tee -a /etc/ssh/sshd_config.d/10-hardening.conf && sudo sshd -t && sudo systemctl reload sshd`.
- **Verified without root:** `ssh -o PreferredAuthentications=password -o PubkeyAuthentication=no fernandovmedina@localhost true` answers `Permission denied (publickey,gssapi-keyex,gssapi-with-mic)` — the daemon offers publickey only.
- **Keys:** `~/.ssh/id_ed25519_jarvis_agent_personal` and `~/.ssh/id_ed25519_jarvis_agent_neurovix`, mapped to the `github_jarvis_agent_personal` / `github_jarvis_agent_neurovix` host aliases in `~/.ssh/config`.
- **Trap:** sshd directives must never live in `/etc/ssh/ssh_config.d/`. The ssh *client* parses that entire directory, aborts on `Bad configuration option: permitrootlogin`, and every outgoing ssh on the machine dies — git over ssh, `gh`, `scp` — while `git push` reports it as an auth/repository error instead of a config error.

## AI agent

- **Framework:** Hermes Agent (Nous Research), installed in `~/.hermes`, CLI at `~/.local/bin/hermes`.
- **Service:** systemd user unit `hermes-gateway.service` (`hermes gateway run`), `Restart=always`, linger enabled so it starts at boot.
  - Override: `~/.config/systemd/user/hermes-gateway.service.d/override.conf` clears the old broken `EnvironmentFile=`.
  - Logs: `journalctl --user -u hermes-gateway -f`.
- **Secrets:** `~/.hermes/.env` (mode 600). Contains DeepSeek, OpenRouter and OpenAI keys; only DeepSeek is in use.
- **Models:** primary `deepseek-v4-flash` (DeepSeek API). The fallback is currently also `deepseek-v4-flash`; the intended fallback is `deepseek-v4-pro`.
- **Model history:** OpenAI `gpt-6-luna` hit its quota (no payment method). OpenRouter free models were rate-limited / ran out. Switched to DeepSeek.
- **Persona:** `~/.hermes/SOUL.md` points to `~/.agent`.
- **Working directory:** `terminal.cwd` in `~/.hermes/config.yaml` = `/home/fernandovmedina/.agent`.
- **Chat history and memory:** stored by Hermes in `~/.hermes/state.db` and `~/.hermes/memories/`.
- **Messaging:** Telegram gateway connected.

### Context directory: `~/.agent`

| Path | Purpose |
|---|---|
| `AGENTS.md` | Entry point; tells Hermes which files to read |
| `context/global/AGENT.md` | Role, responsibilities, memory rules |
| `context/global/OWNER.md` | Owner profile and preferences (answered) |
| `context/global/TOOLS.md` | Available tools and their status |
| `rules/RULES.md` | What needs approval, what is allowed, what is never allowed |
| `skills/portfolio-sync/SKILL.md` | Portfolio workflow; symlinked into `~/.hermes/skills/` |
| `context/chatting/chat.db` | Empty, unused (Hermes keeps its own history) |

## Integrations

Everything Jarvis can reach outside the server, and where each credential lives. Google is the only third-party account it holds credentials for; no voice provider is connected yet.

| Integration | What it does | Credential / config | Status |
|---|---|---|---|
| DeepSeek API | Model inference, `deepseek-v4-flash` | `DEEPSEEK_API_KEY` in `~/.hermes/.env` | Active, primary |
| OpenRouter and OpenAI API | Alternative providers | `OPENROUTER_API_KEY`, `OPENAI_API_KEY` in `~/.hermes/.env` | Configured, unused |
| Telegram | Messaging gateway (`hermes-gateway.service`) | `TELEGRAM_BOT_TOKEN`, `TELEGRAM_HOME_CHANNEL`; only the owner's DM is allowed | Active |
| GitHub MCP | Read-only repo access, exposed as `mcp__github__*` tools | Local stdio server `~/.local/bin/github-mcp-stdio` | Active |
| GitHub CLI | Writes: branches, commits, pushes, PRs, issues | `gh` with both accounts (`fernandovmedina`, `neurovix`) plus the SSH host aliases | Active |
| context7 MCP | Up-to-date library documentation | `https://mcp.context7.com/mcp` | Active |
| Google Workspace | Gmail, Calendar, Drive, Sheets, Docs, Contacts | OAuth: `~/.hermes/google_token.json` + `~/.hermes/google_client_secret.json` | Active, one account |


Not part of this list:

- **Cloudflare.** Only the outbound tunnel, configured under `Network → Cloudflare Tunnel`. It authenticates with its tunnel token (`/etc/cloudflared/token`), not with a Cloudflare API token, and it is what makes `jarvis.fervazquezdev.com` reach SSH from outside the home network.
- **Supabase.** The Minuteko projects carry their own MCP config in their repos (`minuteko-backend/.mcp.json`). That belongs to those projects, not to Jarvis, and nothing of it is loaded into Hermes here.

### Google Workspace

- **Account:** `fernandovazquez.favm@gmail.com` (personal). The business account is not connected yet.
- **Scopes granted:** `gmail.readonly`, `gmail.modify`, `gmail.send`, `calendar`, `drive`, `spreadsheets`, `documents`, `contacts.readonly`.
- **Access path:** the `google-workspace` skill drives `~/.hermes/skills/productivity/google-workspace/scripts/google_api.py`. The token refreshes on its own; if a call fails with `NOT_AUTHENTICATED` or `REFRESH_FAILED`, run `setup.py --check` and redo the OAuth exchange.
- **Verified working:** Gmail search and read, Calendar list, Drive search/upload/export, Sheets create and update, Docs create and edit (used to build the Minuteko client quotes).
- **Not granted:** Maps, Cloud Storage, YouTube, Tasks — no scopes for them, so those calls cannot work until the OAuth flow is re-run with the extra scopes.

## Console dashboard

`jarvis-term` is a full-screen TUI that shows what the agent is doing without opening the agent's own TUI: Telegram traffic, model and tool calls, hardware, addresses and logs. Green and white only, ASCII borders, no icons.

| | |
|---|---|
| Binary | `/usr/sbin/jarvis-term` (`root:root` 0755, 2.7 MB stripped), on `PATH` as `jarvis-term` |
| Source | `terminal/` in this repo — Rust 2021, `ratatui` 0.29, `rusqlite` (bundled), `libc`; single thread, no async runtime |
| Build cache | `~/.cache/jarvis-term` (`cargo build --release`, Rust 1.100) |
| Version check | `jarvis-term --version` → `jarvis-term 0.1.0` |
| Start | manual only: `jarvis-term` on tty1 or over SSH as `fernandovmedina`. No unit, no autostart |
| Env | `HERMES_HOME` overrides `~/.hermes` |

Panels, sources and refresh (each panel keeps at most 500 entries):

| Panel | Shows | Source | Refresh |
|---|---|---|---|
| Header | hostname, IPv4 per interface, uptime, load, clock | `/proc/sys/kernel/hostname`, `getifaddrs`, `/proc/uptime`, `/proc/loadavg` | 1 s / 10 s |
| SYSTEM | CPU total and per core, RAM, swap, temperatures, disk `/`, network rate, battery, services | `/proc/stat`, `/proc/meminfo`, hwmon `coretemp`, `statvfs`, `/proc/net/dev`, `/sys/class/power_supply`, `systemctl is-active` | 1 s (disk, battery and services 10 s) |
| API CALLS | every model call (model, provider, tokens in/out, latency) and every tool call (ok / ERROR) plus today's totals | `~/.hermes/logs/agent.log` | 0.5 s |
| TELEGRAM | owner and Jarvis messages, wrapped | `~/.hermes/state.db`, opened read-only, `source = 'telegram'` | 2 s |
| LOGS | gateway activity and every warning/error | `~/.hermes/logs/gateway.log`, `~/.hermes/logs/errors.log` | 0.5 s |

Services map to units: `gateway` → `hermes-gateway.service` (user unit), `tunnel` → `cloudflared.service`, `sshd` → `sshd.service`. Anything other than `active` renders in bold.

Keys: `q` / `Esc` / `Ctrl+C` quit; `Tab`, `Shift+Tab`, `1 2 3` focus API CALLS / TELEGRAM / LOGS; `Up`/`Down` (`k`/`j`) and `PgUp`/`PgDn` scroll; `Home` (`g`) and `End` (`G`) jump to the oldest line or follow the newest. A scrolled panel stops following and says so in its title.

Cost: about 9 MB RSS and roughly 1 % of one core; a 2.7 MB binary with SQLite compiled in, so no `sqlite-devel` needed at runtime.

```bash
cd terminal
cargo test --release
cargo build --release
sudo install -m 0755 target/release/jarvis-term /usr/sbin/jarvis-term
```

Traps:

- `~/.hermes/logs` is mode 700, so it must run as the user that owns `~/.hermes`; under any other account the API and LOGS panels stay empty.
- `acpitz` reports -269 °C on this hardware and is ignored; only hwmon `coretemp` is shown.
- The 1366×768 console font is tiny: `sudo setfont ter-v22n` for the session (not permanent yet).
- Rendering verified 2026-11-23 by capturing a frame from a pty: header, SYSTEM, API CALLS, TELEGRAM and LOGS all painted with live data.

## Tech stack

| Area | Tools |
|---|---|
| Agent | Hermes Agent, DeepSeek API |
| Messaging | Telegram gateway |
| MCP | GitHub (local, read-only) and context7 (HTTP) |
| Voice | ElevenLabs and/or Twilio (to be chosen) |
| Code | GitHub via two SSH identities: `github_jarvis_agent_personal`, `github_jarvis_agent_neurovix`; `gh` CLI with both accounts |
| Google | Gmail, Calendar, Drive, Sheets, Docs, Contacts via OAuth; personal account connected, business account pending |
| Data | SQLite (finances, local data) |
| Containers | Docker |
| Remote access | Cloudflare Tunnel (`cloudflared` 2026.11.1) → `jarvis.fervazquezdev.com`, gated by Cloudflare Access |
| Portfolio | Next.js + Tailwind CSS, hosted on Vercel, domain `fervazquezdev.com` (bought at Hostinger, DNS on Cloudflare) |

## Work remaining

- **Network:** review firewalld rules; decide whether the tunnel should expose anything besides SSH (e.g. a webhook endpoint for GitHub pushes).
- **Models:** set `deepseek-v4-pro` as the fallback instead of a second `deepseek-v4-flash`; remove unused OpenAI/OpenRouter keys from `~/.hermes/.env`; delete the old `~/secrets` folder.
- **Context files:** update `TOOLS.md` (still lists the old OpenAI model); remove `chat.db` and its mention in `AGENT.md`; restart the gateway and confirm it answers as Jarvis.
- **GitHub:** log in both accounts in `gh`; set up GitHub webhooks (through the tunnel) or a scheduled check so Jarvis sees new pushes; give the portfolio-sync skill the portfolio repo name.
- **Google:** the personal account is connected (Gmail, Calendar, Drive, Sheets, Docs, Contacts); connect the business account when a task needs it and create the finance Google Sheet plus the local SQLite schema.
- **Assistant behavior:** morning briefing, 60/30/5-minute event reminders, agenda change alerts (Hermes cron jobs).
- **Voice:** choose and connect a voice API (ElevenLabs / Twilio) for speaking and calls.
- **Server care:** set `HandleLidSwitch=ignore` so closing the lid doesn't suspend the server; make the Terminus console font permanent; verify Battery Limit keeps the battery at ~50%; check periodically for battery swelling; back up `~/.agent` and `~/.hermes`.
