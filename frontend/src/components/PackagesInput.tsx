import { useRef, useState } from "react";
import { Check, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "cn";

/** Mirrors the backend's package-name pattern (src/models/packages.rs). */
const PACKAGE_RE =
  /^[a-zA-Z0-9][a-zA-Z0-9+._-]*(:[a-zA-Z0-9-]+)?(=[a-zA-Z0-9][a-zA-Z0-9+._:~-]*)?$/;

/** Shared with the backend's MAX_PACKAGES cap (src/models/packages.rs). */
const MAX_PACKAGES = 100;

/** One-click suggestions shown under the field (homelab essentials). */
export const COMMON_PACKAGES = [
  "qemu-guest-agent",
  "curl",
  "htop",
  "vim",
  "build-essential",
];

/**
 * Parses a stored package list the same way the backend does: split on
 * newlines, commas and whitespace, dropping empties and duplicates while
 * preserving first-seen order.
 */
export function parsePackageList(text: string): string[] {
  const seen: string[] = [];
  for (const entry of text.split(/[\n\r, \t]+/)) {
    const trimmed = entry.trim();
    if (trimmed && !seen.includes(trimmed)) seen.push(trimmed);
  }
  return seen;
}

/**
 * Tag/chip input for apt package lists: type a name and press Enter (or a
 * comma) to add a chip; each chip removes with one click on its X; Backspace
 * on an empty field pops the last chip. Invalid or duplicate names are
 * rejected inline with a message instead of being silently accepted.
 */
export function PackagesInput({
  value,
  onChange,
  id,
  disabled,
  suggestions = COMMON_PACKAGES,
}: {
  value: string[];
  onChange: (next: string[]) => void;
  id?: string;
  disabled?: boolean;
  suggestions?: string[];
}) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  // defensive: the form may mount before its `values` are applied (RHF then
  // hands the Controller's raw undefined here)
  const list = value ?? [];

  function add(entry: string) {
    if (list.includes(entry)) {
      setError(`${entry} is already in the list.`);
      return;
    }
    if (!PACKAGE_RE.test(entry)) {
      setError(`"${entry}" is not a valid package name.`);
      return;
    }
    if (list.length >= MAX_PACKAGES) {
      setError(`At most ${MAX_PACKAGES} packages are allowed.`);
      return;
    }
    setError(null);
    onChange([...list, entry]);
  }

  function commit() {
    const entry = draft.trim();
    if (!entry) return;
    add(entry);
    setDraft("");
  }

  function remove(entry: string) {
    setError(null);
    onChange(list.filter((p) => p !== entry));
    inputRef.current?.focus();
  }

  return (
    <div className="flex flex-col gap-1.5">
      <div
        role="listbox"
        aria-label={id ? `${id} packages` : "Packages"}
        onClick={() => inputRef.current?.focus()}
        className={cn(
          "border-input bg-transparent flex min-h-9 w-full cursor-text flex-wrap items-center gap-1.5 rounded-lg border px-2.5 py-1.5",
          "focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50",
          error && "border-destructive ring-destructive/20 focus-within:ring-destructive/20",
          disabled && "cursor-not-allowed opacity-50",
        )}
      >
        {list.map((entry) => (
          <Badge key={entry} variant="outline" className="font-mono">
            {entry}
            {!disabled && (
              <button
                type="button"
                aria-label={`Remove ${entry}`}
                className="hover:text-destructive -mr-1 cursor-pointer"
                onClick={(e) => {
                  e.stopPropagation();
                  remove(entry);
                }}
              >
                <X data-icon="inline-end" />
              </button>
            )}
          </Badge>
        ))}
        <input
          ref={inputRef}
          id={id}
          type="text"
          disabled={disabled}
          value={draft}
          placeholder={list.length === 0 ? "curl" : ""}
          className="placeholder:text-muted-foreground min-w-24 flex-1 bg-transparent text-sm outline-none"
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === ",") {
              e.preventDefault();
              commit();
            } else if (e.key === "Backspace" && draft === "" && list.length > 0) {
              remove(list[list.length - 1]);
            } else if (e.key === "Escape") {
              setDraft("");
              setError(null);
            }
          }}
          onBlur={() => {
            // a half-typed valid name commits on leave, an invalid one is
            // dropped — never silently accepted into the list
            if (draft.trim() && PACKAGE_RE.test(draft.trim()) && !list.includes(draft.trim())) {
              commit();
            }
          }}
        />
      </div>
      {!disabled && suggestions.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-muted-foreground text-xs">Common:</span>
          {suggestions.map((entry) => {
            const added = list.includes(entry);
            return (
              <Button
                key={entry}
                type="button"
                variant="outline"
                size="sm"
                className="h-6 px-2 font-mono text-xs"
                title={added ? `${entry} is in the list` : `Add ${entry}`}
                disabled={added}
                onClick={() => add(entry)}
              >
                {added && <Check data-icon="inline-start" />}
                {entry}
              </Button>
            );
          })}
        </div>
      )}
      {error ? (
        <p role="alert" className="text-destructive text-xs">
          {error}
        </p>
      ) : disabled ? (
        <p className="text-muted-foreground text-xs">
          Locked — reset the machine to edit packages.
        </p>
      ) : (
        <p className="text-muted-foreground text-xs">
          Press Enter to add. Empty falls back to the default packages.
        </p>
      )}
    </div>
  );
}
