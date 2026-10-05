# iPXE ZTP Server

![Rust](https://img.shields.io/badge/rust-2021%20edition-DEA584?logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-MIT-3DA639)
![Loco](https://img.shields.io/badge/loco--rs-1.2-E43717)
![Axum](https://img.shields.io/badge/axum-0.8-0B7285)
![SeaORM](https://img.shields.io/badge/sea--orm-2.0-5B4FC4)
![React](https://img.shields.io/badge/react-19-61DAFB?logo=react&logoColor=white)
![TypeScript](https://img.shields.io/badge/typescript-5-3178C6?logo=typescript&logoColor=white)
![Tailwind CSS](https://img.shields.io/badge/tailwindcss-4-06B6D4?logo=tailwindcss&logoColor=white)
![pnpm](https://img.shields.io/badge/pnpm-10-F9AD00?logo=pnpm&logoColor=white)
![mise](https://img.shields.io/badge/mise-runner-B87514?logo=mise&logoColor=white)

A [Loco](https://loco.rs)-based Rust server for zero-touch provisioning: machines
PXE-boot into iPXE, fetch a boot script keyed by their MAC address, and get an OS
installed automatically after admin approval in the web UI.

## Quick Start

Run with [mise](https://mise.jdx.dev) (tasks defined in [.mise.toml](.mise.toml), see `mise tasks`):

```sh
mise run dev
```

- Frontend dev server: `http://<lan-ip>:5173` (e.g. `http://192.168.250.202:5173`)
- Backend API: `http://<lan-ip>:5150` (e.g. `http://192.168.250.202:5150`)

For a production-like run (one server on port `5150` serving the built frontend, no hot reload):

```sh
mise run serve
```

## dnsmasq / iPXE setup (OpenWrt)

Machines discover the ZTP server through `dnsmasq` (e.g. on an OpenWrt router).
Instead of handing out a plain `pxelinux.0`, point DHCP clients at the ZTP
server's boot endpoint, which returns an iPXE script keyed by the machine's
MAC address:

```
# /etc/config/dhcp (OpenWrt)
config dnsmasq
    ...
    list dhcp_boot 'ipxe,http://192.168.250.202:5150/ipxe/boot?mac=${mac}'
```

Or in a plain `dnsmasq.conf`:

```
dhcp-boot=ipxe,http://192.168.250.202:5150/ipxe/boot?mac=${mac}
```

Replace `192.168.250.202` with the LAN IP of the host running this app
(port `5150` by default). On OpenWrt, `${mac}` is expanded by dnsmasq —
restart dnsmasq after the change (`/etc/init.d/dnsmasq restart`).
The URL must be exactly `/ipxe/boot` — anything else falls through to the
admin UI and iPXE fails with "Exec format error".

## Boot flow

1. A machine PXE-boots and is handed iPXE, which fetches
   `http://192.168.250.202:5150/ipxe/boot?mac=<mac>` (LAN IP of the server).
2. Unknown MAC → registered as `pending`, iPXE loops waiting for approval.
3. Admin approves the machine in the UI and picks an OS.
4. Next boot request serves the install script (kernel + initrd + Ubuntu
   autoinstall); once the installer finishes it calls back and the machine
   flips to `installed`.
5. After that, machines must boot from their own disk — and the boot order
   can stay net-first for the machine's whole lifetime: installed machines
   get a bare `sanboot --no-describe --drive 0x80` script. On UEFI, iPXE's
   SAN boot runs the disk's fallback loader (`\EFI\BOOT\BOOTX64.EFI`), and
   on BIOS it boots the int-13h disk directly; if a build can do neither,
   the failed script exits iPXE with an EFI error status and the boot
   manager falls through to the disk. (Never `exit` from that script on
   UEFI: it returns EFI_SUCCESS, which stops this boot manager instead of
   falling through — the machine would sit on the firmware front page, a
   black screen.) A wiped disk falls back into iPXE on its own, so
   re-provisioning is zero-touch: flip the machine back to pending in the
   UI and reset it.

## Boot modes

Per OS version, chosen in the UI (OS Versions → boot mode button cycles
online → offline → nfs):

- **online** — casper downloads the ISO from the remote mirror into the
  client's RAM (`netboot=url`). Needs RAM ≥ ISO size + ~1.5 GiB.
- **offline** — same, but the ISO comes from this server's downloaded
  copy. Still RAM-bound: a 2 GiB machine cannot hold a 2.9 GiB ISO.
- **nfs** — casper mounts the extracted `casper/` directory (squashfs,
  size, manifest) directly over NFS (`netboot=nfs nfsroot=…`). The medium
  is never copied into RAM, so a **1 GiB** client can install.

## NFS setup (low-RAM installs)

The machine serving NFS must export the data dir; the app cannot do that
itself (needs root). Two supported placements:

### On the Proxmox VE host (recommended — always on, independent of the Mac)

```sh
ssh root@<pve> 'bash -s' < utils/proxmox/setup-nfs-pve.sh   # creates /srv/ipxe-rs + export
mise run sync-nfs                                           # rsync data/ → /srv/ipxe-rs (uses PVE_HOST from the credentials file)
```

`mise run sync-nfs` must be re-run after downloading a new OS version.

### On the machine running the ZTP server

```sh
utils/nfs/setup-nfs-export.sh          # print the export line + steps
sudo utils/nfs/setup-nfs-export.sh --write   # macOS/Linux: configure nfsd
DATA_DIR=/srv/ztp CLIENT_NET=192.168.250.0/24 utils/nfs/setup-nfs-export.sh
```

### Settings

Then in **Settings** set *NFS host* (the IP clients mount from) and *NFS
export root* (when the exported path differs from the data dir). The boot
script's `nfsroot` is `<nfs_host>:<export root>/os/<id>`, whose `casper/`
directory is extracted from the ISO at download time.

## Proxmox credentials (API)

Proxmox API credentials for creating/managing test VMs live in
`secret/proxmox-credentials.env` (gitignored — never commit real secrets).
Create it from the checked-in example:

```sh
cp secret/proxmox-credentials.env.example secret/proxmox-credentials.env
```

Then fill in your PVE host, node, and API token (generate in the Proxmox web
GUI: Datacenter → Permissions → API Tokens → Add).

## Proxmox test VM

`utils/proxmox/create-vm.sh` (run it on a Proxmox VE host) creates a
diskless, network-boot test VM that drops straight into iPXE:

```sh
VMID=999 BRIDGE=vmbr0 ./utils/proxmox/create-vm.sh
qm start 999
```
