#!/usr/bin/env bash
#
# install.sh — reproduce the Jarvis server on a fresh Fedora machine.
#
# Run it as your normal user (the one that will own Hermes), NOT as root.
# It asks for sudo when it needs it. Every step is idempotent: running it
# twice is safe, it just skips what is already done.
#
#   bash installation/install.sh
#
# Things it CANNOT do for you (there is no way to script a human):
#   - create the Cloudflare tunnel and paste its token (you get it from the
#     Zero Trust dashboard, see step 8)
#   - log in to GitHub twice (step 11) and to Google (step 10)
#   - pick the model / paste API keys for Hermes (step 6)
# Those are printed at the end as a checklist.

set -euo pipefail

# ---------------------------------------------------------------- helpers ---

say()  { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }
info() { printf '    %s\n' "$*"; }
die()  { printf '\n\033[1;31mERROR: %s\033[0m\n' "$*" >&2; exit 1; }

# Everything below assumes we are the human user, not root: root's home and
# root's systemd --user instance are not the ones that run the agent.
if [[ $EUID -eq 0 ]]; then
    die "run this as your user (it will call sudo when it needs to), not as root"
fi

# Tunables — override from the environment if your names differ.
HERMES_HOME="${HERMES_HOME:-$HOME/.hermes}"
AGENT_HOME="$HOME/.agent"
WORKSPACE="${WORKSPACE:-$HOME/workspace}"
GO_VERSION="${GO_VERSION:-1.27.4}"
TIMEZONE="${TIMEZONE:-America/Monterrey}"

say "Target"
info "user:     $USER"
info "home:     $HOME"
info "Hermes:   $HERMES_HOME"
info "context:  $AGENT_HOME"
info "Go:       $GO_VERSION"
info "TZ:       $TIMEZONE"

# ------------------------------------------------------------- 1. packages ---
# Base tooling. Fedora 44 ships all of these. `fd-find` is the package name;
# the binary it installs is plain `fd`. sqlite3 is for the chat DB and the
# finance journal, ripgrep/jq because every shell session ends up using them.
say "1/12 Base packages"
sudo dnf install -y \
    git gh jq ripgrep fd-find sqlite3 gcc gcc-c++ make \
    alsa-utils uv curl tar acl
info "installed: $(git --version | cut -d' ' -f3), gh $(gh --version | head -1 | cut -d' ' -f3), jq $(jq --version | cut -d- -f2)"

# --------------------------------------------------------------- 2. Go ---
# Fedora's golang package lags the version Minuteko targets, so Go goes in
# from the official tarball under /usr/local/go. It is not on PATH by design:
# call it as /usr/local/go/bin/go, or export the path in your shell rc.
say "2/12 Go $GO_VERSION"
if [[ -x /usr/local/go/bin/go ]]; then
    info "already present: $(/usr/local/go/bin/go version)"
else
    tmp=$(mktemp -d)
    curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz" -o "$tmp/go.tgz"
    sudo rm -rf /usr/local/go
    sudo tar -C /usr/local -xzf "$tmp/go.tgz"
    rm -rf "$tmp"
    info "installed: $(/usr/local/go/bin/go version)"
fi

# ------------------------------------------------------------ 3. Docker ---
# Docker comes from Docker's own repository, not Fedora's: the distro package
# is podman-adjacent and the compose plugin is missing.
say "3/12 Docker"
if command -v docker >/dev/null 2>&1; then
    info "already present: $(docker --version)"
else
    sudo dnf install -y dnf-plugins-core
    sudo dnf config-manager addrepo --from-repofile=https://download.docker.com/linux/fedora/docker-ce.repo
    sudo dnf install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
fi
sudo systemctl enable --now docker
info "docker: $(systemctl is-active docker)"

# ---------------------------------------------------------- 4. firewall ---
# The only inbound port is SSH. The stock FedoraServer zone also allows
# cockpit, and cockpit.socket is disabled anyway, so drop the allowance to
# keep the rule set honest. dhcpv6-client stays: it is the client side of
# IPv6 addressing, not an open port.
say "4/12 firewalld"
sudo dnf install -y firewalld
sudo systemctl enable --now firewalld
sudo firewall-cmd --permanent --add-service=ssh >/dev/null
sudo firewall-cmd --permanent --add-service=dhcpv6-client >/dev/null
sudo firewall-cmd --permanent --remove-service=cockpit >/dev/null 2>&1 || true
sudo firewall-cmd --reload >/dev/null
sudo firewall-cmd --list-services
info "check the result: it must say 'dhcpv6-client ssh' and nothing else"

# ------------------------------------------------------- 5. sshd hardening ---
# Passwords and keyboard-interactive auth off, keys only.
#
# The trap that cost an afternoon once: these are *sshd* directives, so they
# belong in /etc/ssh/sshd_config.d/. If you drop them in ssh_config.d/ the ssh
# *client* parses them, dies with "Bad configuration option" and every
# outgoing ssh on the machine breaks (git push looks like an auth error).
say "5/12 sshd hardening"
sudo tee /etc/ssh/sshd_config.d/10-hardening.conf >/dev/null <<'EOF'
# keys only: no password and no keyboard-interactive authentication
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin no
EOF
sudo chmod 600 /etc/ssh/sshd_config.d/10-hardening.conf
sudo sshd -t                 # validate BEFORE reloading, or you lock yourself out
sudo systemctl reload sshd
sudo sshd -T | grep -iE '^(passwordauthentication|kbdinteractiveauthentication|permitrootlogin)'
info "keep a second ssh session open until you have confirmed key login still works"

# --------------------------------------------------------- 6. Hermes Agent ---
# The official installer pulls Python, the launcher and the bundled Node/pnpm
# into $HOME/.hermes. It is interactive on purpose: it asks for the model and
# the API keys.
say "6/12 Hermes Agent"
if command -v hermes >/dev/null 2>&1 || [[ -x "$HOME/.local/bin/hermes" ]]; then
    info "already installed: $(hermes --version 2>/dev/null | head -1)"
else
    curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash
fi
mkdir -p "$HERMES_HOME"

# Hermes keeps settings in config.yaml and secrets in .env — never mix them.
# .env must be 600 or the gateway refuses to start.
if [[ ! -f "$HERMES_HOME/.env" ]]; then
    cat > "$HERMES_HOME/.env" <<'EOF'
# Secrets only. Filled by `hermes setup` or by hand, one KEY=value per line.
# Model provider
DEEPSEEK_API_KEY=
# Messaging
TELEGRAM_BOT_TOKEN=
EOF
fi
chmod 600 "$HERMES_HOME/.env"
info "now run: hermes setup   (model + provider + API keys), then: hermes doctor"

# ------------------------------------------------ 7. gateway as a service ---
# systemd *user* unit plus linger, so the gateway comes up at boot without a
# login session. The override clears the installer's EnvironmentFile, which
# pointed at a file that no longer exists.
say "7/12 gateway service"
mkdir -p "$HOME/.config/systemd/user/hermes-gateway.service.d"
cat > "$HOME/.config/systemd/user/hermes-gateway.service.d/override.conf" <<'EOF'
[Service]
EnvironmentFile=
EOF
systemctl --user daemon-reload
if systemctl --user list-unit-files hermes-gateway.service >/dev/null 2>&1; then
    systemctl --user enable --now hermes-gateway.service
else
    info "unit not created yet — the Hermes installer normally writes it."
    info "if it is missing, re-run: hermes gateway install"
fi
sudo loginctl enable-linger "$USER"
info "gateway: $(systemctl --user is-active hermes-gateway.service) | linger: $(loginctl show-user "$USER" -p Linger --value)"

# ------------------------------------------------------------- 8. tunnel ---
# Cloudflare Tunnel: outbound only, no router port forwarding. The token comes
# from Zero Trust -> Networks -> Tunnels and is stored by cloudflared itself in
# /etc/cloudflared/token (root 0600). Ingress and the Access policy are
# configured in the dashboard, not on the server.
say "8/12 Cloudflare Tunnel"
if command -v cloudflared >/dev/null 2>&1; then
    info "already present: $(cloudflared --version)"
else
    sudo dnf install -y cloudflared || {
        info "cloudflared is not in Fedora's repos on every release; using Cloudflare's RPM:"
        sudo dnf install -y https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-x86_64.rpm
    }
fi
if [[ -f /etc/cloudflared/token ]]; then
    info "tunnel token already present"
else
    info "MANUAL: sudo cloudflared service install <TUNNEL_TOKEN>"
    info "then:   sudo systemctl enable --now cloudflared"
    info "        journalctl -u cloudflared -f   # wait for 4 'Registered tunnel connection' lines"
fi

# ------------------------------------------------------------- 9. audio ---
# Fedora Server has no audio stack at all: the kernel sees the codec, nothing
# can drive it. alsa-utils gives aplay/amixer/speaker-test/alsactl and no daemon.
# The Master control ships at 0 with the switch off, which silences the box
# while Speaker still reads 127 — set it and persist it with alsactl.
say "9/12 audio"
if amixer -c 1 sget Master >/dev/null 2>&1; then
    amixer -c 1 sset Master 80% unmute >/dev/null
    amixer -c 1 sset Speaker 80% unmute >/dev/null 2>&1 || true
    sudo alsactl store
    info "speaker-test -D hw:1,0 -c2 -t sine -f 440 -l1   # you should hear it"
else
    info "no PCH card found (card 1): this machine may have no internal speaker"
fi
# Optional: let the agent user write to the PCM device without joining 'audio'.
# NOTE the ACL does not survive a reboot; add a udev rule if you need it.
if [[ -e /dev/snd/pcmC1D0p ]]; then
    sudo setfacl -m "u:$USER:rw" /dev/snd/pcmC1D0p /dev/snd/controlC1 2>/dev/null || true
    info "ACL set on the PCM device (not persistent across reboots)"
fi

# ---------------------------------------------------------- 10. Google ---
# OAuth, one account, for Gmail/Calendar/Drive/Sheets/Docs/Contacts.
# Non-interactive setup is impossible: you need a browser to approve the app.
say "10/12 Google Workspace (manual)"
info "hermes setup google     # or follow the google-workspace skill: setup.py --check"
info "token lands in $HERMES_HOME/google_token.json and refreshes itself"

# ------------------------------------------------------------ 11. GitHub ---
# Two identities: the personal account authors every commit and PR; the
# company account only reviews and merges (that merge triggers the Vercel
# deploy). SSH aliases keep the keys apart.
say "11/12 GitHub"
mkdir -p "$HOME/.ssh"
chmod 700 "$HOME/.ssh"
for key in id_ed25519_jarvis_agent_personal id_ed25519_jarvis_agent_neurovix; do
    if [[ ! -f "$HOME/.ssh/$key" ]]; then
        info "missing $HOME/.ssh/$key — create it with: ssh-keygen -t ed25519 -C \"$key\" -f ~/.ssh/$key"
    fi
done
if ! grep -q 'github_jarvis_agent_personal' "$HOME/.ssh/config" 2>/dev/null; then
    cat >> "$HOME/.ssh/config" <<'EOF'

# PERSONAL GITHUB — authors every commit and PR
Host github_jarvis_agent_personal
    HostName github.com
    User git
    IdentityFile ~/.ssh/id_ed25519_jarvis_agent_personal
    IdentitiesOnly yes

# COMPANY GITHUB — reviews and merges only
Host github_jarvis_agent_neurovix
    HostName github.com
    User git
    IdentityFile ~/.ssh/id_ed25519_jarvis_agent_neurovix
    IdentitiesOnly yes
EOF
    chmod 600 "$HOME/.ssh/config"
fi
info "then, for each account: gh auth login   (and gh auth switch --user <name>)"

# --------------------------------------------------------- 12. workspace ---
# Same layout everywhere: ~/workspace/<account>/<web|apps>/<repo>, so skills
# and the agent never have to guess where a clone lives.
say "12/12 workspace and context skeleton"
mkdir -p "$WORKSPACE"/{fernandovmedina,neurovix}/{web,apps}
mkdir -p "$AGENT_HOME"/{context/global,context/chatting,rules,skills,scripts,state}

# The agent's identity files. The real content lives in your backup; these are
# the minimum that makes a fresh box answer as Jarvis instead of as a generic
# assistant.
if [[ ! -f "$HERMES_HOME/SOUL.md" ]]; then
    echo "You are Jarvis. Your full role and rules are in $AGENT_HOME (start with AGENTS.md)." > "$HERMES_HOME/SOUL.md"
fi
if [[ ! -f "$AGENT_HOME/AGENTS.md" ]]; then
    cat > "$AGENT_HOME/AGENTS.md" <<'EOF'
Before anything else, read and follow:
- context/global/AGENT.md
- context/global/OWNER.md
- context/global/TOOLS.md
- rules/RULES.md
Skills are in skills/.
EOF
fi
info "restore the full ~/.agent content from your backup (tar -xzf agent-backup-*.tgz -C ~)"

# ---------------------------------------------------------------- done ---

say "What is left, by hand"
cat <<EOF
  1. hermes setup                       # model, provider, API keys
  2. hermes doctor                      # green across the board
  3. sudo cloudflared service install <TUNNEL_TOKEN>
     sudo systemctl enable --now cloudflared
  4. gh auth login                      # once per GitHub account
  5. Google OAuth, per the google-workspace skill
  6. hermes mcp add github  --command \$HOME/.local/bin/github-mcp-stdio
     hermes mcp add context7 --url https://mcp.context7.com/mcp
  7. restore ~/.agent from the backup, then: reload the gateway service from a shell of your own

Verify everything with the checklist at the end of installation/README.md.
EOF

say "Base install finished"
info "nothing here touched your secrets: tokens and OAuth are still yours to paste"
