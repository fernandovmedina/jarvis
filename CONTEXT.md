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
| IPv4 | `192.168.0.84/24` via DHCP |
| Gateway | `192.168.0.1` |
| IPv6 | Yes (ISP-assigned, SLAAC) |
| Firewall | firewalld active, zone `FedoraServer`: only `ssh` + `dhcpv6-client`, no extra ports or rich rules |
| SSH | sshd active, publickey only (password and keyboard-interactive refused); root login enabled on purpose, key-only |
| Remote access | Cloudflare Tunnel (`cloudflared` 2026.11.1): `jarvis.fervazquezdev.com` → SSH, behind Cloudflare Access |

From the Mac, `ssh jarvis` connects without a password (`~/.ssh/config`: `Host jarvis` → `192.168.0.84`, user `fernandovmedina`).

The server has a DHCP address on Wi-Fi; it needs a fixed address (router DHCP reservation or static IP) so `ssh jarvis` keeps working. Remote access from outside the home goes through a Cloudflare Tunnel, which also lets GitHub webhooks reach the server for the portfolio workflow without opening router ports.

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

## Tech stack

| Area | Tools |
|---|---|
| Agent | Hermes Agent, DeepSeek API |
| Messaging | Telegram |
| Voice | ElevenLabs and/or Twilio (to be chosen) |
| Code | GitHub via two SSH identities: `github_jarvis_agent_personal`, `github_jarvis_agent_neurovix`; `gh` CLI with both accounts |
| Google | Gmail, Calendar, Drive, Sheets; personal and business accounts |
| Data | SQLite (finances, local data) |
| Containers | Docker |
| Remote access | Cloudflare Tunnel (`cloudflared` 2026.11.1) → `jarvis.fervazquezdev.com`, gated by Cloudflare Access |
| Portfolio | Next.js + Tailwind CSS, hosted on Vercel, domain `fervazquezdev.com` (bought at Hostinger, DNS on Cloudflare) |

## Work remaining

- **Network:** fixed IP for the server; review firewalld rules; decide whether the tunnel should expose anything besides SSH (e.g. a webhook endpoint for GitHub pushes).
- **Models:** set `deepseek-v4-pro` as the fallback instead of a second `deepseek-v4-flash`; remove unused OpenAI/OpenRouter keys from `~/.hermes/.env`; delete the old `~/secrets` folder.
- **Context files:** update `TOOLS.md` (still lists the old OpenAI model); remove `chat.db` and its mention in `AGENT.md`; restart the gateway and confirm it answers as Jarvis.
- **GitHub:** log in both accounts in `gh`; set up GitHub webhooks (through the tunnel) or a scheduled check so Jarvis sees new pushes; give the portfolio-sync skill the portfolio repo name.
- **Google:** connect Gmail, Calendar, Drive and Sheets for both accounts; create the finance Google Sheet and local SQLite schema.
- **Assistant behavior:** morning briefing, 60/30/5-minute event reminders, agenda change alerts (Hermes cron jobs).
- **Voice:** choose and connect a voice API (ElevenLabs / Twilio) for speaking and calls.
- **Server care:** set `HandleLidSwitch=ignore` so closing the lid doesn't suspend the server; make the Terminus console font permanent; verify Battery Limit keeps the battery at ~50%; check periodically for battery swelling; back up `~/.agent` and `~/.hermes`.
