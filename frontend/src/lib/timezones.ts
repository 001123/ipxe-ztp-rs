/// GMT-offset choices for the installed system. The stored value is the
/// matching IANA `Etc/GMT±X` zone so autoinstall/subiquity accepts it. The
/// sign is inverted (POSIX-style): UTC+7 is stored as `Etc/GMT-7`.
///
/// Shared between the global settings page and the per-machine drawer.
export const GMT_OPTIONS: { value: string; label: string }[] = Array.from(
  { length: 27 }, // offsets -12 … +14
  (_, i) => {
    const offset = i - 12;
    if (offset === 0) return { value: "UTC", label: "UTC (GMT+0)" };
    return {
      value: offset > 0 ? `Etc/GMT-${offset}` : `Etc/GMT+${-offset}`,
      label: `GMT${offset > 0 ? "+" : "-"}${Math.abs(offset)}`,
    };
  },
);

/** Human label for a stored GMT zone, e.g. `Etc/GMT-7` → `GMT+7`. */
export function timezoneLabel(value: string): string {
  if (value === "UTC") return "UTC (GMT+0)";
  const match = /^Etc\/GMT([+-])(\d+)$/.exec(value);
  if (!match) return value;
  // the stored sign is POSIX-inverted: Etc/GMT-7 means UTC+7
  const sign = match[1] === "-" ? "+" : "-";
  return `GMT${sign}${match[2]}`;
}