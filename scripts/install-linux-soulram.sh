#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "This installer must run as root." >&2
  exit 1
fi

PREFIX_LIB="/usr/local/lib/soulkernel"
PROVISION_SH="${PREFIX_LIB}/soulram-provision.sh"
SERVICE_PATH="/etc/systemd/system/soulkernel-soulram-provision.service"
UDEV_RULE_PATH="/etc/udev/rules.d/99-soulkernel-zram.rules"

install -d "${PREFIX_LIB}"

cat > "${PROVISION_SH}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ ! -e /sys/block/zram0 ]]; then
  modprobe zram || true
fi

if [[ ! -e /sys/block/zram0 && -w /sys/class/zram-control/hot_add ]]; then
  echo 1 > /sys/class/zram-control/hot_add || true
fi

if [[ -e /sys/class/block/zram0/dev && ! -e /dev/zram0 ]]; then
  IFS=: read -r major minor < /sys/class/block/zram0/dev
  if [[ -n "${major}" && -n "${minor}" ]]; then
    mknod /dev/zram0 b "${major}" "${minor}" || true
  fi
fi

if [[ -e /dev/zram0 ]]; then
  chown root:root /dev/zram0 || true
  chmod 0660 /dev/zram0 || true
fi
EOF

chmod 0755 "${PROVISION_SH}"

cat > "${SERVICE_PATH}" <<EOF
[Unit]
Description=Provision SoulKernel zRAM backend
DefaultDependencies=no
After=systemd-modules-load.service local-fs.target
Before=multi-user.target
ConditionPathExists=/sys

[Service]
Type=oneshot
ExecStart=${PROVISION_SH}
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF

cat > "${UDEV_RULE_PATH}" <<'EOF'
KERNEL=="zram0", MODE="0660", OWNER="root", GROUP="root"
EOF

udevadm control --reload-rules || true
udevadm trigger --subsystem-match=block --action=add || true
systemctl daemon-reload
systemctl enable --now soulkernel-soulram-provision.service

echo "SoulKernel SoulRAM Linux backend provisioned."
echo "Installed:"
echo "  - ${PROVISION_SH}"
echo "  - ${SERVICE_PATH}"
echo "  - ${UDEV_RULE_PATH}"
echo
echo "Next:"
echo "  1. Reboot if your kernel/module policy requires it."
echo "  2. Verify /sys/block/zram0 and /dev/zram0 exist."
echo "  3. Then launch SoulKernel and activate SoulRAM."
