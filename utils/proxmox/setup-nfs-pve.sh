#!/usr/bin/env bash
# Set up an NFS export on the Proxmox VE host for low-RAM network installs
# (boot mode "nfs").
#
# Why: casper's netboot=url copies the whole ISO into the client's RAM, so
# installing Ubuntu live-server needs roughly ISO-size + 1.5 GiB of RAM.
# NFS boot mounts the extracted casper/ directory directly — a 1 GiB client
# can install. The ZTP server's data dir is rsync'd into NFS_ROOT_DIR
# (mise run sync-nfs) and exported read-only from here.
#
# Usage (on the PVE host, or piped over SSH from your machine):
#   ssh root@<pve> 'bash -s' < setup-nfs-pve.sh
#   NFS_ROOT_DIR=/srv/ipxe-rs CLIENT_NET=192.168.250.0/24 ssh root@<pve> 'bash -s' < setup-nfs-pve.sh
#
# The export dir name (ipxe-rs) marks it as belonging to the ZTP server so
# it is recognizable next to other /srv content.
set -euo pipefail

NFS_ROOT_DIR="${NFS_ROOT_DIR:-/srv/ipxe-rs}"
CLIENT_NET="${CLIENT_NET:-192.168.250.0/24}"

mkdir -p "$NFS_ROOT_DIR"
apt-get update
apt-get install -y nfs-kernel-server

# read-only export of the data dir mirror; insecure lets busybox's nfsmount
# (in the client initramfs) connect from an unprivileged source port
EXPORT_LINE="$NFS_ROOT_DIR $CLIENT_NET(ro,no_subtree_check,insecure)"
if grep -q "^[[:space:]]*$NFS_ROOT_DIR[[:space:]]" /etc/exports 2>/dev/null; then
  echo "export for $NFS_ROOT_DIR already present in /etc/exports"
else
  echo "$EXPORT_LINE" >> /etc/exports
fi

exportfs -ra
systemctl enable --now nfs-server

echo
echo "NFS export ready:"
showmount -e localhost || exportfs -v
echo
echo "Next steps:"
echo "  1. Sync the ZTP server data dir from your workstation:"
echo "       mise run sync-nfs        # (or: rsync -av --delete data/ root@<pve>:$NFS_ROOT_DIR/)"
echo "  2. ZTP server UI → Settings:"
echo "       NFS host        = <IP clients reach the PVE host on>"
echo "       NFS export root = $NFS_ROOT_DIR"