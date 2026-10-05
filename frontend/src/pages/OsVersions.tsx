import {
  AlertTriangle,
  Check,
  Disc3,
  Download,
  Globe,
  HardDriveDownload,
  Server,
  X,
} from "lucide-react";
import type { OsVersionResponse } from "@/bindings/OsVersionResponse";
import { useOsVersions, useUpdateOsVersion } from "@/api/hooks";
import { ApiClientError } from "@/api/client";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { toast } from "sonner";

type DownloadStatus = "pending" | "downloading" | "ready" | "failed";

const STATUS_CLASSES: Record<DownloadStatus, string> = {
  pending: "bg-chart-2/15 text-chart-2 border-chart-2/30",
  downloading: "bg-chart-3/15 text-chart-3 border-chart-3/30",
  ready: "bg-chart-4/15 text-chart-4 border-chart-4/30",
  failed: "bg-destructive/10 text-destructive border-destructive/30",
};

function DownloadStatusBadge({ status }: { status: string }) {
  const classes = STATUS_CLASSES[status as DownloadStatus];
  return (
    <Badge variant="outline" className={classes}>
      {status}
    </Badge>
  );
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  const index = Math.min(units.length - 1, Math.floor(Math.log2(bytes) / 10));
  const value = bytes / 1024 ** index;
  return `${value.toFixed(value >= 100 || index === 0 ? 0 : 1)} ${units[index]}`;
}

function ProgressCell({ os }: { os: OsVersionResponse }) {
  const percent =
    os.total_bytes > 0 ? Math.round((os.bytes_downloaded / os.total_bytes) * 100) : null;

  return (
    <div className="flex w-48 flex-col gap-1">
      {/* Base UI progress is indeterminate when value is null. */}
      <Progress value={percent} aria-label="Download progress" />
      <span className="text-muted-foreground text-xs tabular-nums">
        {formatBytes(os.bytes_downloaded)}
        {os.total_bytes > 0 ? ` / ${formatBytes(os.total_bytes)}` : ""}
      </span>
    </div>
  );
}

type BootMode = "online" | "offline" | "nfs";

const BOOT_MODE_ORDER: BootMode[] = ["online", "offline", "nfs"];

const BOOT_MODE_META: Record<BootMode, { icon: typeof Globe; classes: string; blurb: string }> = {
  online: {
    icon: Globe,
    classes: "border-chart-3/30 bg-chart-3/15 text-chart-3",
    blurb: "casper downloads the ISO from the remote mirror into the client's RAM",
  },
  offline: {
    icon: HardDriveDownload,
    classes: "border-chart-4/40 bg-chart-4/15 text-chart-4",
    blurb: "casper downloads the locally mirrored ISO into the client's RAM",
  },
  nfs: {
    icon: Server,
    classes: "border-chart-5/30 bg-chart-5/15 text-chart-5",
    blurb:
      "casper mounts the extracted casper dir over NFS — no ISO copy in RAM, works on low-RAM machines",
  },
};

function BootModeCell({ os, onToggle }: { os: OsVersionResponse; onToggle: () => void }) {
  const mode = (os.boot_mode as BootMode) ?? "online";
  const meta = BOOT_MODE_META[mode] ?? BOOT_MODE_META.online;
  const next = BOOT_MODE_ORDER[(BOOT_MODE_ORDER.indexOf(mode) + 1) % BOOT_MODE_ORDER.length];
  const needsDownload =
    (mode === "offline" || mode === "nfs") && os.download_status !== "ready";
  const Icon = meta.icon;

  return (
    <div className="flex flex-col items-start gap-1">
      <Button
        variant="ghost"
        size="sm"
        className={`h-6 gap-1.5 rounded-full border px-2.5 ${meta.classes}`}
        title={`Switch to ${next} boot: ${BOOT_MODE_META[next].blurb}`}
        onClick={onToggle}
      >
        <Icon className="size-3.5" />
        {mode}
      </Button>
      {needsDownload && (
        <span className="text-destructive text-xs">download required</span>
      )}
    </div>
  );
}

const BOOT_MODE_GUIDE: Record<
  BootMode,
  { description: string; pros: string[]; cons: string[] }
> = {
  online: {
    description:
      "casper downloads the ISO from the remote mirror straight into the client's RAM.",
    pros: [
      "No server-side storage — nothing to download or sync",
      "Always installs the latest release from the mirror",
      "Works for machines anywhere with internet access",
    ],
    cons: [
      "The whole ISO lives in the client's RAM: ~3 GiB ISO + ~1.5 GiB installer",
      "Needs 4 GiB+ RAM clients",
      "Fails without internet at install time",
    ],
  },
  offline: {
    description:
      "Same flow as online, but the ISO is served from this server's downloaded copy.",
    pros: [
      "Installs work without internet access",
      "ISO transfers at LAN speed instead of the client's internet link",
      "Pinned local copy — reproducible installs per OS version",
    ],
    cons: [
      "Same RAM cost as online — a 2 GiB machine cannot hold a ~3 GiB ISO",
      "Requires downloading and storing the ISO on the server (~3 GiB per OS version)",
    ],
  },
  nfs: {
    description:
      "casper mounts the extracted casper/ directory over NFS (netboot=nfs) and reads the squashfs straight from the network.",
    pros: [
      "Medium is never copied into RAM — even a 1 GiB machine can install",
      "No per-client ISO copy; reads are served straight from disk",
      "Only the small kernel + initrd cross the wire via HTTP",
    ],
    cons: [
      "Requires the NFS export setup (see the README's NFS setup section)",
      "The NFS host must be online and reachable for the whole install",
      "Re-sync needed after each OS download (mise run sync-nfs)",
    ],
  },
};

function BootModeGuide() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Boot modes</CardTitle>
        <CardDescription>
          How the installer fetches its live medium. Click a boot mode badge
          above to cycle an OS version between the modes.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-5">
        {BOOT_MODE_ORDER.map((mode) => {
          const guide = BOOT_MODE_GUIDE[mode];
          const Icon = BOOT_MODE_META[mode].icon;
          return (
            <div key={mode} className="space-y-2">
              <div className="flex items-start gap-3">
                <div className="bg-muted text-muted-foreground flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border">
                  <Icon className="size-4" />
                </div>
                <div className="min-w-0">
                  <div className="text-sm font-medium">{mode}</div>
                  <p className="text-muted-foreground text-sm leading-relaxed">
                    {guide.description}
                  </p>
                </div>
              </div>
              <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <div className="text-xs font-medium tracking-wide uppercase">
                    Pros
                  </div>
                  {guide.pros.map((item) => (
                    <div key={item} className="flex items-start gap-2">
                      <Check className="text-chart-1 mt-0.5 size-3.5 shrink-0" />
                      <span className="text-muted-foreground text-sm">{item}</span>
                    </div>
                  ))}
                </div>
                <div className="space-y-1.5">
                  <div className="text-xs font-medium tracking-wide uppercase">
                    Cons
                  </div>
                  {guide.cons.map((item) => (
                    <div key={item} className="flex items-start gap-2">
                      <X className="text-destructive mt-0.5 size-3.5 shrink-0" />
                      <span className="text-muted-foreground text-sm">{item}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          );
        })}
      </CardContent>
    </Card>
  );
}

export function OsVersions() {
  const { data, isLoading } = useOsVersions();
  const updateOsVersion = useUpdateOsVersion();

  const osVersions = data?.items ?? [];
  const anyInFlight = osVersions.some(
    (os) => os.download_status === "pending" || os.download_status === "downloading",
  );

  function toggleBootMode(os: OsVersionResponse) {
    const order: BootMode[] = ["online", "offline", "nfs"];
    const current = (os.boot_mode as BootMode) ?? "online";
    const next = order[(order.indexOf(current) + 1) % order.length];
    updateOsVersion.mutate(
      {
        id: String(os.id),
        params: {
          name: null,
          version: null,
          arch: null,
          kernel_url: null,
          initrd_url: null,
          iso_url: null,
          boot_mode: next,
        },
      },
      {
        onSuccess: () => {
          toast.success(`${os.name} ${os.version} now boots ${next}`);
        },
        onError: (err) =>
          toast.error(
            err instanceof ApiClientError ? err.message : "Failed to change boot mode",
          ),
      },
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">OS Versions</h1>
        <p className="text-muted-foreground text-sm">
          Operating systems available for auto-install, with mirror download
          status and per-OS boot mode (online / offline / nfs).
        </p>
      </div>

      {isLoading ? (
        <div className="space-y-2 rounded-xl border p-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} className="h-10 w-full" />
          ))}
        </div>
      ) : osVersions.length === 0 ? (
        <Empty className="rounded-xl border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Disc3 />
            </EmptyMedia>
            <EmptyTitle>No OS versions</EmptyTitle>
            <EmptyDescription>
              OS versions are seeded by the server (Ubuntu 24.04 / 26.04).
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="space-y-6">
          {anyInFlight && (
            <Alert>
              <Download />
              <AlertTitle>Downloads in progress or queued</AlertTitle>
              <AlertDescription>
                The list refreshes automatically every 2 seconds while artifacts
                are being fetched from the mirror.
              </AlertDescription>
            </Alert>
          )}
          <div className="rounded-xl border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>OS</TableHead>
                  <TableHead>Arch</TableHead>
                  <TableHead>Download</TableHead>
                  <TableHead>Progress</TableHead>
                  <TableHead>Boot mode</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {osVersions.map((os) => {
                  const inFlight =
                    os.download_status === "pending" ||
                    os.download_status === "downloading";
                  return (
                    <TableRow key={String(os.id)}>
                      <TableCell>
                        <span className="font-medium">
                          {os.name} {os.version}
                        </span>
                      </TableCell>
                      <TableCell>{os.arch}</TableCell>
                      <TableCell>
                        <DownloadStatusBadge status={os.download_status} />
                      </TableCell>
                      <TableCell>
                        {inFlight ? (
                          <ProgressCell os={os} />
                        ) : os.download_status === "failed" ? (
                          <span className="text-destructive flex items-center gap-1 text-xs">
                            <AlertTriangle className="size-3.5" />
                            {os.error ?? "Download failed"}
                          </span>
                        ) : os.download_status === "ready" ? (
                          <span className="text-muted-foreground text-xs tabular-nums">
                            {formatBytes(os.total_bytes)}
                          </span>
                        ) : null}
                      </TableCell>
                      <TableCell>
                        <BootModeCell os={os} onToggle={() => toggleBootMode(os)} />
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </div>
          <BootModeGuide />
        </div>
      )}
    </div>
  );
}
