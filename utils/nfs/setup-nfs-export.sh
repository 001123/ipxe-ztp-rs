#!/usr/bin/env bash
# Help the operator NFS-export the ZTP server's data dir for low-RAM
# network installs (boot mode "nfs").
#
# Why: casper's netboot=url copies the whole ISO into the client's RAM, so
# installing Ubuntu live-server needs roughly ISO-size + 1.5 GiB of RAM.
# NFS boot mounts the extracted casper/ directory directly — a 1 GiB client
# can install. See the README's "NFS setup" section.
#
# Usage:
#   ./setup-nfs-export.sh                      # print the export line + steps
#   ./setup-nfs-export.sh --write              # also append to /etc/exports
#   DATA_DIR=/srv/ztp CLIENT_NET=10.0.0.0/24 ./setup-nfs-export.sh
#
# Run on the machine hosting the ZTP server (macOS or Linux). Needs root
# for --write.
set -euo pipefail

WRITE=0
if [ "${1:-}" = "--write" ]; then
  WRITE=1
fi

# Default to the repo's data dir; the server resolves its own data dir from
# `settings.data_dir` in the config or the platform data directory.
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DATA_DIR="${DATA_DIR:-$PROJECT_DIR/data}"
CLIENT_NET="${CLIENT_NET:-192.168.250.0}"
CLIENT_MASK="${CLIENT_MASK:-255.255.255.0}"

if [ ! -d "$DATA_DIR/os" ]; then
  echo "warning: $DATA_DIR/os does not exist yet — download an OS in the ZTP UI first" >&2
fi

if [ "$WRITE" = 1 ] && [ "$(id -u)" != "0" ]; then
  echo "error: --write needs root (sudo)" >&2
  exit 1
fi

OS="$(uname -s)"
EXPORT_LINE=""

if [ "$OS" = "Darwin" ]; then
  # macOS nfsd: -ro is enough since casper only reads; the export mirrors
  # the data dir, whose os/<id>/casper/ subtrees the boot script points at.
  EXPORT_LINE="$DATA_DIR -ro -network $CLIENT_NET -mask $CLIENT_MASK"
  echo "Export line for /etc/exports:"
  echo "  $EXPORT_LINE"
  echo
  echo "Steps:"
  echo "  1. sudo sh -c 'echo \"$EXPORT_LINE\" >> /etc/exports'"
  echo "  2. sudo nfsd enable && sudo nfsd restart"
  echo "  3. Verify: showmount -e localhost"
  if [ "$WRITE" = 1 ]; then
    grep -qF "$DATA_DIR" /etc/exports 2>/dev/null || echo "$EXPORT_LINE" >> /etc/exports
    nfsd enable && nfsd restart
    echo "Enabled nfsd and wrote /etc/exports."
  fi
else
  EXPORT_LINE="$DATA_DIR $CLIENT_NET(ro,fsid=0,no_subtree_check,insecure)"
  echo "Export line for /etc/exports:"
  echo "  $EXPORT_LINE"
  echo
  echo "Steps (Debian/Ubuntu):"
  echo "  1. sudo apt install nfs-kernel-server   # if missing"
  echo "  2. sudo sh -c 'echo \"$EXPORT_LINE\" >> /etc/exports'"
  echo "  3. sudo exportfs -ra && sudo systemctl enable --now nfs-server"
  echo "  4. Verify: showmount -e localhost"
  if [ "$WRITE" = 1 ]; then
    grep -qF "$DATA_DIR" /etc/exports 2>/dev/null || echo "$EXPORT_LINE" >> /etc/exports
    exportfs -ra
    systemctl enable --now nfs-server
    echo "Wrote /etc/exports and reloaded exportfs."
  fi
fi

echo
echo "In the ZTP server UI → Settings, set:"
echo "  NFS export root: $DATA_DIR   (only if you exported a different path than the data dir)"
echo "  NFS host:        <IP clients reach this server on>   (defaults to the public host)"
echo
echo "Then switch the OS version to boot mode 'nfs'. Clients need no extra RAM"
echo "for the live medium: casper mounts the squashfs straight over NFS."