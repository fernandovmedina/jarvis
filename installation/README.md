# Installation

Reproduction procedure for the Jarvis host: a headless Fedora Server machine that runs
[Hermes Agent](https://github.com/NousResearch/hermes-agent) as a systemd user service, exposes
nothing to the network except SSH through a Cloudflare Tunnel, and keeps its identity, permissions
and procedures in `~/.agent`.

The companion `install.sh` executes the automatable part of this document. This file is the
reference: what each step does, why, and how to verify it.

---

## 1. Target

| | |
|---|---|
| OS | Fedora Server 44, x86_64 |
| Kernel | 7.3.x |
| Hostname | `jarvis-agent` |
| Disk | 117 GB LVM, single `/` (13 GB used on a working install) |
| RAM / swap | 3.7 GB / 3.7 GB — no local inference possible |
| Console | TTY only, no desktop session |
| Network | DHCP on Wi-Fi, carrier NAT upstream, no inbound ports |
| Timezone | `America/Monterrey` (UTC-6) |

Prerequisites that cannot be installed: a Cloudflare zone with a named tunnel, one or more GitHub
accounts, an API key for the model provider, and a Telegram bot.

## 2. Reference versions

Versions verified on the running host; pin them when reproducing.

| Component | Version | Source |
|---|---|---|
| Fedora | 44 (kernel 7.3.4) | ISO |
| git | 2.56.1 | dnf |
| gh | 2.97.0 | dnf |
| sqlite3 | 3.51.2 | dnf |
| jq | 1.8.1 | dnf |
| ripgrep | 15.2.0 | dnf |
| fd | 10.5.2 | dnf (`fd-find`) |
| gcc | 16.2.1 | dnf |
| uv | 0.13.7 | dnf |
| Go | 1.27.4 | tarball → `/usr/local/go` |
| Docker | 29.10.0 | `docker-ce-stable` repo |
| cloudflared | 2026.11.1 | dnf / Cloudflare RPM |
| alsa-utils | 1.2.16 | dnf |
| Hermes Agent | 0.23.3 | official installer |
| Node | 26.12.1 | bundled under `~/.hermes/tools` |
| pnpm | 12.15.0 | bundled under `~/.hermes/tools` |

Node and pnpm are not system packages: the Hermes installer manages them inside `~/.hermes/tools`
and puts them on the service `PATH`. Do not install a second Node from dnf — the agent picks the
bundled one.

## 3. Filesystem layout

```
~/.agent/                       agent context (identity, rules, skills, scripts)
├── AGENTS.md                   entry point: which files to read, in which order
├── context/global/             AGENT.md, OWNER.md, TOOLS.md
├── context/chatting/           chat mirror DB (chat.db) if enabled
├── rules/RULES.md              ask-first list, allowed actions, prohibitions
├── skills/<name>/SKILL.md      procedures, symlinked into ~/.hermes/skills
├── scripts/                    wakeup_bomb.sh, phone_sound_reminder.sh
└── state/                      runtime flags and logs (wakeup.lock, wakeup-stop)

~/.hermes/                      Hermes runtime
├── config.yaml                 settings (no secrets), ~135 KB with defaults
├── .env                        secrets only, mode 600
├── SOUL.md                     one line pointing at ~/.agent
├── google_token.json           OAuth for Google Workspace, auto-refreshing
├── google_client_secret.json   OAuth client (Desktop app)
├── state.db                    canonical session store (SQLite + FTS5)
├── logs/                       agent.log, errors.log, gateway.log
├── scripts/                    scripts referenced by cron jobs
└── skills/                     installed skills

~/workspace/<account>/<web|apps>/<repo>    clones, one layout for every account
```

## 4. Base packages

```bash
sudo dnf install -y git gh jq ripgrep fd-find sqlite3 gcc gcc-c++ make alsa-utils uv curl tar acl
```

`fd-find` provides the `fd` binary. `uv` is packaged in Fedora 44; if it is missing on a later
release, install it from astral.sh instead.

### Go

Fedora's `golang` lags Minuteko's target. Install the tarball and use the absolute path:

```bash
curl -fsSL https://go.dev/dl/go1.27.4.linux-amd64.tar.gz -o /tmp/go.tgz
sudo rm -rf /usr/local/go && sudo tar -C /usr/local -xzf /tmp/go.tgz
/usr/local/go/bin/go version
```

`/usr/local/go/bin` is deliberately *not* on `PATH`; call the binary by path or export it per shell.

### Docker

```bash
sudo dnf install -y dnf-plugins-core
sudo dnf config-manager addrepo --from-repofile=https://download.docker.com/linux/fedora/docker-ce.repo
sudo dnf install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
sudo systemctl enable --now docker
```

## 5. Hermes Agent

```bash
curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash
hermes setup          # model, provider, API keys
hermes doctor
```

Layout and invariants:

| Item | Detail |
|---|---|
| CLI | `~/.local/bin/hermes` (launcher) → `~/.hermes/hermes-agent/` (source) |
| Settings | `~/.hermes/config.yaml`; change with `hermes config set KEY VALUE`, never by hand |
| Secrets | `~/.hermes/.env`, mode `600`; keys used here: `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `OPENAI_API_KEY`, `TELEGRAM_BOT_TOKEN`, `TELEGRAM_HOME_CHANNEL`, `TELEGRAM_ALLOWED_USERS` |
| Persona | `~/.hermes/SOUL.md` → one line pointing at `~/.agent` |
| Working directory | `terminal.cwd = /home/<user>/.agent` in `config.yaml` |
| Sessions | `~/.hermes/state.db` (SQLite + FTS5) |
| Logs | `~/.hermes/logs/{agent,errors,gateway}.log`; `hermes logs -f` tails them |

### Gateway service

Installed by the Hermes installer as a **user** unit. It must survive logout, so linger is enabled.

```bash
systemctl --user enable --now hermes-gateway.service
sudo loginctl enable-linger "$USER"
systemctl --user status hermes-gateway.service
```

The unit file is `~/.config/systemd/user/hermes-gateway.service` and sets
`HERMES_HOME`, `HERMES_SUPERVISED_CHILD=1` and the bundled-Node `PATH`. A drop-in clears the
installer's `EnvironmentFile=`, which points at a file that does not exist:

```ini
# ~/.config/systemd/user/hermes-gateway.service.d/override.conf
[Service]
EnvironmentFile=
```

Logs: `journalctl --user -u hermes-gateway -f`.

## 6. Agent context (`~/.agent`)

The context directory is the product: the model is interchangeable, these files are not. Minimum
set to make a fresh box answer as Jarvis:

```
AGENTS.md                  read order for everything below
context/global/AGENT.md    role, responsibilities, memory rules
context/global/OWNER.md    owner profile, preferences
context/global/TOOLS.md    what is connected
rules/RULES.md             ask-first list and hard prohibitions
skills/                    at least portfolio-sync; symlinked into ~/.hermes/skills
```

Restore the full tree from a backup (`tar -xzf agent-backup-*.tgz -C ~`) and verify the symlink
from `~/.hermes/skills/<name>` resolves into `~/.agent/skills/<name>`.

## 7. SSH

Two layers: `sshd` hardened to keys-only, and a client config with one host alias per GitHub
identity.

### Server side

```bash
sudo tee /etc/ssh/sshd_config.d/10-hardening.conf >/dev/null <<'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin no
EOF
sudo chmod 600 /etc/ssh/sshd_config.d/10-hardening.conf
sudo sshd -t && sudo systemctl reload sshd
```

Verify without root:

```bash
ssh -o PreferredAuthentications=password -o PubkeyAuthentication=no "$USER"@localhost true
# → Permission denied (publickey,gssapi-keyex,gssapi-with-mic)
```

> **Do not put these directives in `/etc/ssh/ssh_config.d/`.** That directory is parsed by the ssh
> *client*, which aborts with `Bad configuration option: permitrootlogin` and breaks every outgoing
> ssh on the machine — `git push` then fails as an auth error, not a config error.

### Client side

`~/.ssh/config` (mode 600) maps one alias per identity:

```
Host github_jarvis_agent_personal
    HostName github.com
    User git
    IdentityFile ~/.ssh/id_ed25519_jarvis_agent_personal
    IdentitiesOnly yes

Host github_jarvis_agent_neurovix
    HostName github.com
    User git
    IdentityFile ~/.ssh/id_ed25519_jarvis_agent_neurovix
    IdentitiesOnly yes
```

Verify each alias reaches the right account: `ssh -T git@github_jarvis_agent_personal`.

## 8. Firewall

firewalld with the `FedoraServer` zone. Inbound: `ssh` only, plus `dhcpv6-client` (client side of
IPv6 addressing, not an open port).

```bash
sudo firewall-cmd --permanent --add-service=ssh
sudo firewall-cmd --permanent --add-service=dhcpv6-client
sudo firewall-cmd --permanent --remove-service=cockpit   # allowed by default, unused here
sudo firewall-cmd --reload
sudo firewall-cmd --list-all
```

Expected: `services: dhcpv6-client ssh`, no ports, no rich rules, `masquerade: no`. Forwarding is
moot — `net.ipv4.ip_forward = 0` and `net.ipv6.conf.all.forwarding = 0`.

## 9. Cloudflare Tunnel

Outbound-only connectivity; no router port forwarding, no inbound firewall hole.

```bash
sudo cloudflared service install <TUNNEL_TOKEN>
sudo systemctl enable --now cloudflared
journalctl -u cloudflared -f      # wait for four "Registered tunnel connection" lines
```

| Item | Value |
|---|---|
| Mode | Remotely managed, token-based — **no** `config.yml` |
| Token | `/etc/cloudflared/token`, `root:root 0600`, written by `cloudflared service install` |
| Unit | `/etc/systemd/system/cloudflared.service`, `Restart=on-failure`, enabled |
| Ingress | configured in Zero Trust → Networks → Tunnels, `hostname → ssh://localhost:22` + catch-all `http_status:404` |
| Protection | Cloudflare Access in front of the hostname; a plain HTTP request returns `302` to the Access login |
| Client | `ssh -o ProxyCommand="cloudflared access ssh --hostname %h" <user>@<hostname>` |

The effective ingress is visible in the journal as `Updated to new configuration config="..."`.

## 10. Audio

Fedora Server ships no userspace audio at all. `alsa-utils` provides `aplay`, `amixer`,
`speaker-test` and `alsactl`, with no daemon.

```bash
aplay -l                                  # card 1 = PCH/Realtek (speakers), card 0 = HDMI
amixer -c 1 sset Master 80% unmute
amixer -c 1 sset Speaker 80% unmute
sudo alsactl store                        # → /var/lib/alsa/asound.state
speaker-test -D hw:1,0 -c2 -t sine -f 440 -l1
```

`alsa-restore.service` is `WantedBy=sound.target`, so the stored mixer state is restored at boot.

> **Silence trap:** `Master Playback Volume` sits after the per-output amps. A fresh install has it
> at `0` with the switch off, which mutes the box while `Speaker` still reads 127. Confirm the
> hardware amp, not the control: `grep -A3 'Speaker Playback Volume' /proc/asound/card1/codec#0`
> should show a raised `Amp-Out vals: [0x4b 0x4b]`, not `[0x00 0x00]`.

Write access for the agent user uses an ACL on the PCM device, not group membership:

```bash
sudo setfacl -m "u:$USER:rw" /dev/snd/pcmC1D0p /dev/snd/controlC1
```

The ACL is **not** persistent across reboots; make it durable with a udev rule on the `sound`
subsystem if the agent must keep playing sound unattended.

## 11. GitHub identities

`gh` holds both accounts and switches per operation; git identity is set **per repo**.

```bash
git config user.name  fernandovmedina
git config user.email fernandovazquez.favm@gmail.com
```

Policy: every commit and PR is authored by the personal account, including in company repos. The
company account only reviews and merges — that merge is what triggers the Vercel deployment. Never
add a `Co-Authored-By` trailer.

## 12. MCP servers

| Server | Transport | Registration |
|---|---|---|
| GitHub | local stdio, `~/.local/bin/github-mcp-stdio`, read-only | `hermes mcp add github --command ~/.local/bin/github-mcp-stdio` |
| context7 | HTTP, `https://mcp.context7.com/mcp` | `hermes mcp add context7 --url https://mcp.context7.com/mcp` |

Verify with `hermes mcp list` (both `enabled`) and by calling a tool. Writes to GitHub go through
the `gh` CLI, not the MCP server.

## 13. Scheduled jobs

Two cron jobs exist on the reference host. Both are created through Hermes (the `cronjob` tool or
`hermes cron create`), not with system cron.

| Job | Schedule (America/Monterrey) | Behaviour |
|---|---|---|
| Wake-up call | `30 6 * * 1-5` | Launches `~/.agent/scripts/wakeup_bomb.sh` as a transient `systemd --user` unit; the script messages the owner every 75 s for up to 45 min and stops as soon as he replies (it reads inbound messages from `state.db`) |
| Phone on sound | `0 23 * * *` | `no_agent` job; runs `~/.hermes/scripts/phone_sound_reminder.sh` and delivers its stdout verbatim |

```bash
hermes cron list
```

Scripts referenced by cron must live in `~/.hermes/scripts/` (relative path), or be called by
absolute path from an agent job. `systemd-run --user` is how the wake-up job detaches itself from
the cron session so the loop outlives it.

## 14. Manual steps (cannot be scripted)

1. `hermes setup` — model, provider, API keys.
2. `sudo cloudflared service install <TUNNEL_TOKEN>` — token comes from the dashboard.
3. `gh auth login` per account, then `gh auth switch --user <name>`.
4. Google Workspace OAuth (`google-workspace` skill, `setup.py --check` → browser consent).
5. Restore `~/.agent` and restart the gateway.
6. Telegram: create the bot, put the token in `.env`, set `TELEGRAM_ALLOWED_USERS` to the owner.

## 15. Verification checklist

```bash
# system
hostnamectl --static; uname -r
free -h; df -h /

# firewall: ssh only
sudo firewall-cmd --list-all

# ssh: keys only, aliases resolve
sudo sshd -T | grep -iE '^(passwordauthentication|kbdinteractiveauthentication|permitrootlogin)'
ssh -T git@github_jarvis_agent_personal

# tunnel: four registered connections, ingress loaded
systemctl is-active cloudflared
journalctl -u cloudflared --no-pager | grep -c 'Registered tunnel connection'
journalctl -u cloudflared --no-pager | grep 'Updated to new configuration' | tail -1

# agent
hermes doctor
hermes status
hermes logs errors -n 20
hermes mcp list
hermes cron list

# gateway as a user service, starting at boot
systemctl --user is-active hermes-gateway.service
loginctl show-user "$USER" -p Linger

# audio
amixer -c 1 sget Master | tail -1
grep -A3 'Speaker Playback Volume' /proc/asound/card1/codec#0 | grep 'Amp-Out vals'
```

## 16. Known gaps

- **Audio ACL is not persistent.** `/dev/snd/*` ACLs vanish on reboot; a udev rule is required for
  unattended playback.
- **`~/.agent` is not versioned.** Its content lives only in backups; the identity files are the
  single point of failure.
- **No test suite for the shell scripts.** `wakeup_bomb.sh` and `phone_sound_reminder.sh` were
  validated by dry runs (`DRY=1`) and by a kill-switch test, not by automated tests.
- **Docker is installed but the agent does not use it yet**; the daemon is enabled, so containers
  can publish ports and would be affected by the firewall rules, not by the zone's `ssh` allowance.
- **`cloudflared` is not in every Fedora release's repos.** The fallback in `install.sh` installs
  Cloudflare's RPM directly.
