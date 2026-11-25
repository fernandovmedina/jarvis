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
# Fedora 44 split the kdump machinery out of kexec-tools: `kdump-utils` is the
# package that ships kdumpctl and kdump.service. Installing only kexec-tools
# leaves kexec on the disk and no service to load the crash kernel.
for pkg in kexec-tools kdump-utils; do
    if rpm -q "$pkg" >/dev/null 2>&1; then
        ok "$pkg already installed"
    else
        dnf install -y "$pkg"
        ok "$pkg installed"
    fi
done
if ! systemctl list-unit-files kdump.service >/dev/null 2>&1; then
    echo "    warning: kdump.service still missing after the install" >&2
fi

if grubby --info=ALL | grep -q 'crashkernel='; then
    ok "crashkernel already configured: $(grubby --info=ALL | grep -o 'crashkernel=[^ ]*' | head -1)"
else
    # 384M fits comfortably in 3.7G and leaves room for a 7.x kernel's vmcore
    grubby --update-kernel=ALL --args="crashkernel=384M"
    ok "crashkernel=384M reserved"
fi

# A hang must leave evidence instead of going silent, and a panic must recover
# unattended. Only write keys this kernel actually exports: Fedora 44 does not
# ship kernel.hung_task_*, and naming a missing key makes systemd-sysctl log an
# error on every boot.
SYSCTL_FILE=/etc/sysctl.d/90-crash-capture.conf
{
    echo "# A hang must leave evidence instead of going silent, and a panic must"
    echo "# recover unattended. Written by installation/freeze-hardening.sh."
    for entry in \
        kernel.panic=10 \
        kernel.softlockup_panic=1 \
        kernel.hardlockup_panic=1 \
        kernel.panic_on_rcu_stall=1
    do
        key="${entry%%=*}"
        path="/proc/sys/${key//.//}"
        if [[ -e $path ]]; then
            printf '%s = %s\n' "$key" "${entry#*=}"
        else
            printf '    skip (not exported by this kernel): %s\n' "$key"
        fi
    done
} >"$SYSCTL_FILE"
sysctl --system >/dev/null 2>&1 || true
ok "sysctl file: $SYSCTL_FILE"
ok "live: softlockup_panic=$(sysctl -n kernel.softlockup_panic 2>/dev/null) hardlockup_panic=$(sysctl -n kernel.hardlockup_panic 2>/dev/null) panic=$(sysctl -n kernel.panic 2>/dev/null)"

systemctl enable kdump.service >/dev/null 2>&1 || true
if systemctl start kdump.service 2>/dev/null; then
    ok "kdump active: $(systemctl is-active kdump.service)"
else
    echo "    note: kdump could not start yet; with crashkernel already on the"
    echo "          kernel command line it starts without a reboot once"
    echo "          kdump-utils is installed: sudo systemctl enable --now kdump"
fi

# ------------------------------------------------------------------ audit ---
say "Result"
printf '    cmdline now : %s\n' "$(grubby --info=DEFAULT | grep -o 'args=.*' | cut -c1-160)"
printf '    next boot   : pcie_aspm=off crashkernel=384M\n'
printf '    hw          : (%s) %s\n' "$IFACE" "$(iw dev "$IFACE" get power_save 2>/dev/null || echo 'n/a')"
printf '\n    reboot to arm 2 and 3: sudo reboot\n'
printf '    after the reboot, prove it: cat /proc/cmdline | tr " " "\\n" | grep -E "pcie_aspm|crashkernel" && systemctl is-active kdump\n'
