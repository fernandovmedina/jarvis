#!/usr/bin/env bash
#
# freeze-hardening.sh — stop the silent hangs on this Surface Pro 3.
#
# What it changes, and why:
#
#   1. Wi-Fi power save off.  The 88W8897 (mwifiex_pcie) wedges under a burst of
#      broadcast traffic with power save enabled; when it wedges it takes the
#      PCIe path with it and the machine stops logging, stops answering ssh and
#      cannot read binaries ("command not found" on a live console).
#   2. pcie_aspm=off.  Second suspect on this hardware: PCIe active state power
#      management is a classic silent-freeze source on Surface devices.
#   3. Crash capture.  kexec-tools + a reserved crashkernel turn the next hang
#      into a vmcore instead of silence, hung-task detection panics on purpose
#      so it does, and kernel.panic=10 reboots by itself instead of waiting for
#      someone to press the power button.
#   4. Nothing here scans the LAN.  A /24 ping sweep is what preceded the last
#      freeze; use the router's client table or scan from the laptop.
#
# Run as the owner with sudo, then reboot:
#
#   sudo bash installation/freeze-hardening.sh
#   sudo reboot
#
# Idempotent: every step checks before it changes anything.

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "run with sudo: sudo bash $0" >&2
    exit 1
fi

OWNER="${SUDO_USER:-fernandovmedina}"
CONNECTION="${WIFI_CONNECTION:-internet-de-traviesa}"
IFACE="${WIFI_IFACE:-wlp1s0}"

say() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }
ok()  { printf '    ok: %s\n' "$*"; }

# ---------------------------------------------------- 1. Wi-Fi power save ---
say "Wi-Fi power save"
if nmcli -g 802-11-wireless.powersave connection show "$CONNECTION" >/dev/null 2>&1; then
    # 2 = disable, so the driver never enters the state that hangs
    nmcli connection modify "$CONNECTION" 802-11-wireless.powersave 2
    ok "NetworkManager powersave=2 for '$CONNECTION'"
else
    echo "    skip: connection '$CONNECTION' not found (set WIFI_CONNECTION=...)"
fi
if command -v iw >/dev/null 2>&1 && iw dev "$IFACE" info >/dev/null 2>&1; then
    iw dev "$IFACE" set power_save off || true
    ok "runtime: $(iw dev "$IFACE" get power_save)"
fi

# ------------------------------------------------------- 2. PCIe ASPM off ---
say "Kernel arguments"
if grubby --info=ALL | grep -q 'pcie_aspm=off'; then
    ok "pcie_aspm=off already present"
else
    grubby --update-kernel=ALL --args="pcie_aspm=off"
    ok "pcie_aspm=off added to every kernel entry"
fi

# --------------------------------------------------- 3. Capture the crash ---
say "Crash capture"
if ! rpm -q kexec-tools >/dev/null 2>&1; then
    dnf install -y kexec-tools
else
    ok "kexec-tools already installed"
fi

if grubby --info=ALL | grep -q 'crashkernel='; then
    ok "crashkernel already configured: $(grubby --info=ALL | grep -o 'crashkernel=[^ ]*' | head -1)"
else
    # 384M fits comfortably in 3.7G and leaves room for a 7.x kernel's vmcore
    grubby --update-kernel=ALL --args="crashkernel=384M"
    ok "crashkernel=384M reserved"
fi

cat >/etc/sysctl.d/90-crash-capture.conf <<'EOF'
# A hang must leave evidence instead of going silent, and a panic must recover
# unattended. Written by installation/freeze-hardening.sh.
kernel.hung_task_timeout_secs = 60
kernel.hung_task_panic = 1
kernel.softlockup_panic = 1
kernel.panic = 10
EOF
sysctl --system >/dev/null 2>&1 || true
ok "sysctl: hung_task 60s → panic, softlockup → panic, panic → reboot after 10s"
ok "live: $(sysctl -n kernel.hung_task_panic kernel.softlockup_panic kernel.panic | tr '\n' ' ')"

systemctl enable kdump.service >/dev/null 2>&1 || true
if systemctl start kdump.service 2>/dev/null; then
    ok "kdump active"
else
    echo "    note: kdump needs the reboot to find its crashkernel reservation"
fi

# ------------------------------------------------------------------ audit ---
say "Result"
printf '    cmdline now : %s\n' "$(grubby --info=DEFAULT | grep -o 'args=.*' | cut -c1-160)"
printf '    next boot   : pcie_aspm=off crashkernel=384M\n'
printf '    hw          : (%s) %s\n' "$IFACE" "$(iw dev "$IFACE" get power_save 2>/dev/null || echo 'n/a')"
printf '\n    reboot to arm 2 and 3: sudo reboot\n'
printf '    after the reboot, prove it: cat /proc/cmdline | tr " " "\\n" | grep -E "pcie_aspm|crashkernel" && systemctl is-active kdump\n'
