#!/usr/bin/env bash
# Create a network-boot test VM on a Proxmox host for the iPXE/ZTP server.
#
# Usage (run on the Proxmox host, or via SSH from your machine):
#   ./create-vm.sh                       # VMID 999, default bridge
#   VMID=123 BRIDGE=vmbr1 ./create-vm.sh
#   DISK_SIZE=20 ./create-vm.sh          # scratch disk GiB on STORAGE, default 10
#   MAC=BC:24:11:8B:DF:9E ./create-vm.sh # fixed NIC MAC (default: same each run)
#
# The VM has no disk/boot image: it falls straight into iPXE, which is
# handed out by dnsmasq and pointed at the ZTP server's /ipxe/boot URL
# (see the README, "dnsmasq / iPXE setup" section).
set -euo pipefail

VMID="${VMID:-999}"
NAME="${NAME:-ztp-test-vm}"
BRIDGE="${BRIDGE:-vmbr0}"
MEMORY="${MEMORY:-2048}"
CORES="${CORES:-2}"
STORAGE="${STORAGE:-local-lvm}"
DISK_SIZE="${DISK_SIZE:-10}"  # GiB — qm's --scsi0 takes a plain number, no suffix
# Fixed MAC by default: the ZTP server keys machines by MAC, so a stable
# address means destroy + recreate keeps the same machine record / state.
MAC="${MAC:-BC:24:11:8B:DF:9E}"

if ! command -v qm >/dev/null 2>&1; then
  echo "error: 'qm' not found — run this script on a Proxmox VE host (or: ssh root@pve 'bash -s' < create-vm.sh)" >&2
  exit 1
fi

if qm status "$VMID" >/dev/null 2>&1; then
  echo "error: VMID $VMID already exists" >&2
  exit 1
fi

qm create "$VMID" \
  --name "$NAME" \
  --machine q35 \
  --bios ovmf \
  --efidisk0 "${STORAGE}:0,pre-enrolled-keys=0" \
  --memory "$MEMORY" \
  --cores "$CORES" \
  --net0 "virtio=${MAC},bridge=${BRIDGE}" \
  --ostype l26 \
  --agent 0 \
  --serial0 socket

# Boot order is network-first, disk second — and it stays that way for the
# machine's whole lifetime. The initial boot hits an empty disk: OVMF fails
# the HARDDISK entry cleanly and falls through to net0, so the VM
# PXE/iPXE-boots. An installed machine never escapes iPXE either: the boot
# script is a bare `sanboot --no-describe --drive 0x80`, and iPXE's EFI SAN
# boot runs the local disk's fallback loader (\EFI\BOOT\BOOTX64.EFI) — grub
# takes over and the installed OS boots. (If an iPXE build lacks that path,
# the failed script exits iPXE with an EFI error status, and this boot
# manager tries the next boot option on error — same destination.) That is
# why the script must NOT `exit` on UEFI: `exit` returns EFI_SUCCESS and
# OVMF stops instead of falling through — every reboot of an installed
# machine lands on the firmware front page, a black screen that looks hung.
# Net-first also keeps re-provisioning zero-touch: a wiped disk simply
# falls back into iPXE.
# The scratch disk (virtio-scsi → /dev/sda in the guest) is the autoinstall
# target pinned by the ZTP server's curtin config, which is a UEFI layout
# (GPT + ESP) — hence q35 + OVMF above. The efidisk keeps the UEFI boot
# entry so the installed OS boots from disk, and pre-enrolled-keys=0 leaves
# Secure Boot off so the (unsigned) iPXE binary from dnsmasq can run. Note:
# for UEFI clients dnsmasq must serve ipxe.efi (arch 00:07/00:09), not the
# legacy undionly.kpxe chain.
qm set "$VMID" --scsihw virtio-scsi-pci --scsi0 "${STORAGE}:${DISK_SIZE}"
qm set "$VMID" --boot order='net0;scsi0'

echo "Created VM $VMID ($NAME) on ${BRIDGE}, MAC ${MAC}."
echo "Start it with:  qm start $VMID"
echo "It will PXE-boot into iPXE and hit the ZTP server's /ipxe/boot endpoint."
echo
echo "Note (LXC instead of a VM): containers share the host kernel and cannot"
echo "PXE-boot. To exercise the ZTP flow in LXC, boot the container with a"
echo "standard image, then install and run the upstream netboot-aware iPXE"
echo "(e.g. 'ipxe' package) manually against the same /ipxe/boot URL — or just"
echo "use a VM, which is the supported path."
