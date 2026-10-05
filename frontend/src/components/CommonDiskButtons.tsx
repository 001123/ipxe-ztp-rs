import { Button } from "@/components/ui/button";

/** Device paths most machines expose for the OS install. */
const COMMON_DISKS = ["/dev/sda", "/dev/nvme0n1"];

/**
 * Quick-pick buttons for the usual install disk paths, shared by the
 * settings form and the machine drawer.
 */
export function CommonDiskButtons({
  onPick,
  disabled = false,
}: {
  onPick: (disk: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex items-center gap-2">
      <span className="text-muted-foreground text-xs">Common:</span>
      {COMMON_DISKS.map((disk) => (
        <Button
          key={disk}
          type="button"
          variant="outline"
          size="sm"
          className="h-6 px-2 font-mono text-xs"
          disabled={disabled}
          onClick={() => onPick(disk)}
        >
          {disk}
        </Button>
      ))}
    </div>
  );
}
