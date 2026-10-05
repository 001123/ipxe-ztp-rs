import { Badge } from "@/components/ui/badge";

type MachineStatus = "pending" | "approved" | "installing" | "installed" | "failed";

/** Statuses → token classes (DESIGN.md §7: no hardcoded colors). */
const STATUS_CLASSES: Record<MachineStatus, string> = {
  pending: "bg-chart-2/15 text-chart-2 border-chart-2/30",
  approved: "bg-chart-1/15 text-chart-1 border-chart-1/30",
  installing: "bg-chart-3/15 text-chart-3 border-chart-3/30",
  installed: "bg-chart-4/15 text-chart-4 border-chart-4/30",
  failed: "bg-destructive/10 text-destructive border-destructive/30",
};

/** Colored lifecycle badge shared by the machines table and the drawer. */
export function StatusBadge({ status }: { status: string }) {
  const classes = STATUS_CLASSES[status as MachineStatus];
  return (
    <Badge variant="outline" className={classes}>
      {status}
    </Badge>
  );
}
