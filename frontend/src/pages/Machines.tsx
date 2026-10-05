import { useMemo, useState } from "react";
import { ModuleRegistry, AllCommunityModule, themeQuartz } from "ag-grid-community";
import type { ColDef, ICellRendererParams } from "ag-grid-community";
import { AgGridReact } from "ag-grid-react";
import { Check, HardDrive, Pencil, Plus, RotateCcw, Trash2 } from "lucide-react";
import type { MachineResponse } from "@/bindings/MachineResponse";
import type { OsVersionResponse } from "@/bindings/OsVersionResponse";
import {
  useApproveMachine,
  useDeleteMachine,
  useMachines,
  useOsVersions,
  useResetMachine,
} from "@/api/hooks";
import { ApiClientError } from "@/api/client";
import { MachineDrawer } from "@/components/MachineDrawer";
import { StatusBadge } from "@/components/StatusBadge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { formatMac } from "@/lib/utils";
import { toast } from "sonner";

ModuleRegistry.registerModules([AllCommunityModule]);

function StatusCell({ value }: ICellRendererParams<MachineResponse, string>) {
  return <StatusBadge status={value ?? ""} />;
}

function timeAgo(timestamp: string | null): string {
  if (!timestamp) return "—";
  const then = new Date(timestamp).getTime();
  if (Number.isNaN(then)) return "—";
  const seconds = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

function LastSeenCell({ value }: ICellRendererParams<MachineResponse, string | null>) {
  return <span className="text-muted-foreground">{timeAgo(value ?? null)}</span>;
}

/** Compact human duration, e.g. "42s", "12m 30s", "1h 05m". */
function formatDuration(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  if (hours > 0) return `${hours}h ${String(minutes % 60).padStart(2, "0")}m`;
  if (minutes > 0) return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
  return `${seconds}s`;
}

/**
 * Install start → install end (installed), or start → now while still
 * installing. `null` unless the machine has begun installing.
 */
function installDurationMs(machine: MachineResponse): number | null {
  if (!machine.install_started_at) return null;
  const start = new Date(machine.install_started_at).getTime();
  if (Number.isNaN(start)) return null;
  const end =
    machine.status === "installed" && machine.installed_at
      ? new Date(machine.installed_at).getTime()
      : Date.now();
  if (Number.isNaN(end) || end < start) return null;
  return end - start;
}

function InstallTimeCell({ data }: ICellRendererParams<MachineResponse>) {
  if (!data) return null;
  const ms = installDurationMs(data);
  if (ms === null) {
    return <span className="text-muted-foreground">—</span>;
  }
  return (
    <span className="font-mono text-xs">
      {formatDuration(ms)}
      {data.status === "installing" && (
        <span className="text-muted-foreground"> (running)</span>
      )}
    </span>
  );
}

function errorMessage(err: unknown, fallback: string): string {
  return err instanceof ApiClientError ? err.message : fallback;
}

function osLabel(os: OsVersionResponse | undefined): string {
  if (!os) return "—";
  return `${os.name} ${os.version} (${os.arch})`;
}

/** ag-grid theme mapped onto the shadcn design tokens (adapts to dark/night). */
const machineGridTheme = themeQuartz.withParams({
  backgroundColor: "var(--card)",
  foregroundColor: "var(--card-foreground)",
  headerBackgroundColor: "var(--muted)",
  headerTextColor: "var(--muted-foreground)",
  headerFontWeight: 500,
  borderColor: "var(--border)",
  rowHoverColor: "var(--accent)",
  accentColor: "var(--ring)",
  fontFamily: "inherit",
  fontSize: 13,
  borderRadius: 0,
  wrapperBorder: false,
  headerRowBorder: true,
  columnBorder: true,
});

export function Machines() {
  const { data, isLoading } = useMachines();
  const { data: osData } = useOsVersions();
  const approveMachine = useApproveMachine();
  const resetMachine = useResetMachine();
  const deleteMachine = useDeleteMachine();

  const machines = data?.items ?? [];
  const osVersions = useMemo(() => osData?.items ?? [], [osData]);
  const osById = useMemo(() => new Map(osVersions.map((os) => [os.id, os])), [osVersions]);

  const [drawerOpen, setDrawerOpen] = useState(false);
  const [drawerMachine, setDrawerMachine] = useState<MachineResponse | null>(null);
  const [approveTarget, setApproveTarget] = useState<MachineResponse | null>(null);
  const [selectedOsId, setSelectedOsId] = useState<number | null>(null);
  const [resetTarget, setResetTarget] = useState<MachineResponse | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<MachineResponse | null>(null);

  function openEdit(machine: MachineResponse) {
    setDrawerMachine(machine);
    setDrawerOpen(true);
  }

  function openCreate() {
    setDrawerMachine(null);
    setDrawerOpen(true);
  }

  function openApprove(machine: MachineResponse) {
    setApproveTarget(machine);
    // Preselect the default OS from settings, if any.
    setSelectedOsId(null);
  }

  function submitApprove() {
    if (!approveTarget || selectedOsId === null) return;
    approveMachine.mutate(
      { id: String(approveTarget.id), osVersionId: selectedOsId },
      {
        onSuccess: () => {
          toast.success(`Machine ${formatMac(approveTarget.mac)} approved`);
          // chain into the edit drawer so the operator can finish the
          // install identity (hostname, username, SSH key, ...) right away
          const target = approveTarget;
          setApproveTarget(null);
          openEdit(target);
        },
        onError: (err) =>
          toast.error(errorMessage(err, "Failed to approve machine")),
      },
    );
  }

  function submitReset() {
    if (!resetTarget) return;
    resetMachine.mutate(String(resetTarget.id), {
      onSuccess: () => {
        toast.success(`Machine ${formatMac(resetTarget.mac)} reset to pending`);
        setResetTarget(null);
      },
      onError: (err) => toast.error(errorMessage(err, "Failed to reset machine")),
    });
  }

  function submitDelete() {
    if (!deleteTarget) return;
    deleteMachine.mutate(String(deleteTarget.id), {
      onSuccess: () => {
        toast.success(`Machine ${formatMac(deleteTarget.mac)} deleted`);
        setDeleteTarget(null);
      },
      onError: (err) => toast.error(errorMessage(err, "Failed to delete machine")),
    });
  }

  function canApprove(machine: MachineResponse): boolean {
    return machine.status === "pending" || machine.status === "failed";
  }

  function canReset(machine: MachineResponse): boolean {
    return machine.status !== "pending" && machine.status !== "failed";
  }

  function ActionsCell({ data }: ICellRendererParams<MachineResponse>) {
    if (!data) return null;
    return (
      <div className="flex h-full items-center justify-end gap-1">
        <Button
          variant="ghost"
          size="icon-sm"
          title="Edit install identity"
          onClick={() => openEdit(data)}
        >
          <Pencil />
        </Button>
        {canApprove(data) && (
          <Button
            variant="ghost"
            size="icon-sm"
            title="Approve for install"
            onClick={() => openApprove(data)}
          >
            <Check />
          </Button>
        )}
        {canReset(data) && (
          <Button
            variant="ghost"
            size="icon-sm"
            title="Reset to pending"
            onClick={() => setResetTarget(data)}
          >
            <RotateCcw />
          </Button>
        )}
        <Button
          variant="ghost"
          size="icon-sm"
          className="text-destructive hover:text-destructive"
          title="Delete machine"
          onClick={() => setDeleteTarget(data)}
        >
          <Trash2 />
        </Button>
      </div>
    );
  }

  const columnDefs = useMemo<ColDef<MachineResponse>[]>(() => {
    return [
      {
        field: "mac",
        headerName: "MAC",
        cellClass: "font-mono text-xs",
        valueFormatter: (p) => formatMac(p.value),
        flex: 1,
        minWidth: 160,
      },
      {
        field: "name",
        headerName: "Hostname",
        flex: 1,
        minWidth: 120,
        valueFormatter: (p) => p.value ?? "—",
      },
      {
        field: "username",
        headerName: "Username",
        flex: 0.8,
        minWidth: 110,
        valueFormatter: (p) => p.value ?? "—",
      },
      {
        field: "ssh_key",
        headerName: "SSH key",
        flex: 1.4,
        minWidth: 180,
        valueFormatter: (p) => p.value ?? "—",
        tooltipValueGetter: (p) => p.value ?? undefined,
      },
      {
        field: "status",
        headerName: "Status",
        cellRenderer: StatusCell,
        width: 120,
      },
      {
        field: "last_seen_at",
        headerName: "Last seen",
        cellRenderer: LastSeenCell,
        width: 110,
      },
      {
        field: "install_started_at",
        headerName: "Install time",
        colId: "installTime",
        cellRenderer: InstallTimeCell,
        // sort/filter on the numeric duration, not the display string
        valueGetter: (p) => {
          const machine = p.data;
          if (!machine) return null;
          return installDurationMs(machine);
        },
        width: 130,
      },
      {
        headerName: "OS",
        colId: "os",
        width: 190,
        valueGetter: (p) => {
          const machine = p.data;
          if (!machine) return "—";
          const os =
            machine.os_version_id !== null
              ? osById.get(machine.os_version_id)
              : undefined;
          if (os) return osLabel(os);
          return machine.status === "pending" ? "Not approved" : "—";
        },
      },
      {
        headerName: "Actions",
        colId: "actions",
        pinned: "right",
        width: 170,
        sortable: false,
        filter: false,
        cellRenderer: ActionsCell,
      },
    ];
    // ActionsCell closes over the stable handlers of the component
  }, [osById]);

  const defaultColDef = useMemo<ColDef<MachineResponse>>(
    () => ({
      sortable: true,
      resizable: true,
      filter: true,
    }),
    [],
  );

  if (isLoading) {
    return (
      <div className="space-y-6">
        <MachineHeader onCreate={openCreate} />
        <div className="space-y-2 rounded-xl border p-4">
          {Array.from({ length: 5 }).map((_, i) => (
            <Skeleton key={i} className="h-10 w-full" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <MachineHeader onCreate={openCreate} />

      {machines.length === 0 ? (
        <Empty className="rounded-xl border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HardDrive />
            </EmptyMedia>
            <EmptyTitle>No machines yet</EmptyTitle>
            <EmptyDescription>
              Machines appear here automatically the first time they boot via
              iPXE and hit the boot script — or add one manually.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="table-glow h-[calc(100vh-16rem)] min-h-80 overflow-hidden rounded-xl border">
          <AgGridReact
            theme={machineGridTheme}
            columnDefs={columnDefs}
            defaultColDef={defaultColDef}
            rowData={machines}
            getRowId={(p) => String(p.data.id)}
            rowHeight={44}
            headerHeight={40}
            suppressDragLeaveHidesColumns
            overlayNoRowsTemplate="No machines yet"
          />
        </div>
      )}

      <MachineDrawer open={drawerOpen} onOpenChange={setDrawerOpen} machine={drawerMachine} />

      {/* Approve dialog: pick the OS version to install. */}
      <Dialog
        open={approveTarget !== null}
        onOpenChange={(next) => {
          if (!next) setApproveTarget(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Approve machine</DialogTitle>
            <DialogDescription>
              Pick the OS version to install on {formatMac(approveTarget?.mac)}. The
              machine will start installing on its next boot.
            </DialogDescription>
          </DialogHeader>
          <Select
            items={osVersions.map((os) => ({
              value: os.id,
              label: osLabel(os),
            }))}
            value={selectedOsId}
            onValueChange={(value) => setSelectedOsId(value as number | null)}
          >
            <SelectTrigger className="w-full" aria-label="OS version">
              <SelectValue placeholder="Select OS version" />
            </SelectTrigger>
            <SelectContent>
              {osVersions.map((os) => (
                <SelectItem key={String(os.id)} value={os.id}>
                  {osLabel(os)}
                  {os.download_status !== "ready" && (
                    <Badge variant="outline" className="ml-2">
                      {os.download_status}
                    </Badge>
                  )}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setApproveTarget(null)}
              disabled={approveMachine.isPending}
            >
              Cancel
            </Button>
            <Button
              onClick={submitApprove}
              disabled={selectedOsId === null || approveMachine.isPending}
            >
              {approveMachine.isPending ? "Approving…" : "Approve"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Reset confirmation. */}
      <Dialog
        open={resetTarget !== null}
        onOpenChange={(next) => {
          if (!next) setResetTarget(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Reset machine</DialogTitle>
            <DialogDescription>
              Reset {formatMac(resetTarget?.mac)} to <strong>pending</strong>? It will wait
              for approval again on its next boot.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setResetTarget(null)}
              disabled={resetMachine.isPending}
            >
              Cancel
            </Button>
            <Button onClick={submitReset} disabled={resetMachine.isPending}>
              {resetMachine.isPending ? "Resetting…" : "Reset"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete confirmation. */}
      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(next) => {
          if (!next) setDeleteTarget(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete machine</DialogTitle>
            <DialogDescription>
              Permanently delete {formatMac(deleteTarget?.mac)}? This cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setDeleteTarget(null)}
              disabled={deleteMachine.isPending}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={submitDelete}
              disabled={deleteMachine.isPending}
            >
              {deleteMachine.isPending ? "Deleting…" : "Delete"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function MachineHeader({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Machines</h1>
        <p className="text-muted-foreground text-sm">
          Network-boot machines registered by MAC address.
        </p>
      </div>
      <Button onClick={onCreate}>
        <Plus data-icon="inline-start" />
        Add machine
      </Button>
    </div>
  );
}