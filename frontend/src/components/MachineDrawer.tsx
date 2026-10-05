import { useEffect, useState } from "react";
import { Controller, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import type { MachineResponse } from "@/bindings/MachineResponse";
import { useCreateMachine, useSettings, useUpdateMachine } from "@/api/hooks";
import { ApiClientError } from "@/api/client";
import { PackagesInput, parsePackageList } from "@/components/PackagesInput";
import { CommonDiskButtons } from "@/components/CommonDiskButtons";
import { StatusBadge } from "@/components/StatusBadge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { formatMac } from "@/lib/utils";
import { Label } from "@/components/ui/label";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { GMT_OPTIONS, timezoneLabel } from "@/lib/timezones";
import { toast } from "sonner";

/** Mirrors the backend's RFC 1123 hostname pattern (src/models/machines.rs). */
const HOSTNAME_RE =
  /^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$/;

/** A curtin device path, e.g. /dev/sda or /dev/nvme0n1. */
const DISK_RE = /^\/dev\/[A-Za-z0-9./_-]+$/;

// Radix Select forbids empty item values, so the non-timezone choices are
// spelled with sentinels and mapped back to "" at the edges: inherit = use
// the global settings fallback, custom = a stored value outside the GMT list
const TIMEZONE_INHERIT = "__inherit__";
const TIMEZONE_CUSTOM = "__custom__";

const machineFormSchema = z.object({
  mac: z
    .string()
    .trim()
    .min(12, "MAC address must look like a MAC address.")
    .max(32, "MAC address must look like a MAC address."),
  name: z
    .string()
    .trim()
    .max(253, "Hostname is too long.")
    .refine(
      (value) => value === "" || HOSTNAME_RE.test(value),
      "Hostname must be a valid hostname.",
    ),
  username: z.string().trim().max(64, "Username must be at most 64 characters."),
  ssh_key: z.string().trim().max(4096, "SSH key is too long."),
  install_disk: z
    .string()
    .trim()
    .max(64, "Disk path is too long.")
    .refine(
      (value) => value === "" || DISK_RE.test(value),
      "Disk must be a device path like /dev/sda.",
    ),
  cloudinit_url: z
    .string()
    .trim()
    .max(2048, "Cloud-init URL is too long.")
    .refine(
      (value) => value === "" || /^https?:\/\//.test(value),
      "Cloud-init URL must start with http:// or https://.",
    ),
  notes: z.string().trim().max(2000, "Notes are too long."),
  packages: z.array(z.string()).max(100, "At most 100 packages are allowed."),
  timezone: z.string().trim().max(64, "Timezone is too long."),
  password: z.string().max(256, "Password is too long."),
});

type MachineFormValues = z.infer<typeof machineFormSchema>;

/** Empty string means "unset" — the API takes Option fields. */
function toOptional(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

function errorMessage(err: unknown, fallback: string): string {
  return err instanceof ApiClientError ? err.message : fallback;
}

function FieldError({ message }: { message: string | undefined }) {
  if (!message) return null;
  return <p className="text-destructive text-xs">{message}</p>;
}

/**
 * Right-side drawer for editing a machine's install identity, or registering
 * one manually (create mode when `machine` is null). The identity overrides
 * the global settings for that machine's autoinstall.
 */
export function MachineDrawer({
  open,
  onOpenChange,
  machine,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  machine: MachineResponse | null;
}) {
  const createMachine = useCreateMachine();
  const updateMachine = useUpdateMachine();
  const { data: settings } = useSettings();
  const isEdit = machine !== null;
  // an installed machine's identity already did its job — lock the form
  const isInstalled = isEdit && machine.status === "installed";
  const isPending = createMachine.isPending || updateMachine.isPending;
  // the cloud-init toggle is pure UI: unchecking just clears the URL below
  const [useCloudinit, setUseCloudinit] = useState(false);
  // the password input is write-only (the stored hash is never shown);
  // toggling this flags the save to drop the machine's own password and
  // fall back to the global settings password
  const [clearPassword, setClearPassword] = useState(false);

  const {
    register,
    handleSubmit,
    reset,
    setValue,
    getValues,
    control,
    formState: { errors },
  } = useForm<MachineFormValues>({
    resolver: zodResolver(machineFormSchema),
    defaultValues: {
      mac: "",
      name: "",
      username: "",
      ssh_key: "",
      install_disk: "",
      cloudinit_url: "",
      notes: "",
      packages: [],
      timezone: "",
      password: "",
    },
  });

  // each open starts from the machine's current values (or blank in create)
  useEffect(() => {
    if (open) {
      reset({
        mac: machine?.mac ?? "",
        name: machine?.name ?? "",
        username: machine?.username ?? "",
        ssh_key: machine?.ssh_key ?? "",
        install_disk: machine?.install_disk ?? "",
        cloudinit_url: machine?.cloudinit_url ?? "",
        notes: machine?.notes ?? "",
        packages: parsePackageList(machine?.packages ?? ""),
        timezone: machine?.timezone ?? "",
        password: "",
      });
      setUseCloudinit(Boolean(machine?.cloudinit_url));
      setClearPassword(false);
    }
  }, [open, machine, reset]);

  // prefill the SSH key from the global settings when the field is empty:
  // the autoinstall seed falls back to the setting anyway, so showing it up
  // front makes the fallback visible. A machine's own key always wins, and
  // the effect only runs when the drawer opens or settings load — never on
  // keystrokes, so a deliberately cleared field stays cleared.
  useEffect(() => {
    if (!open || !settings?.ssh_key) return;
    if (machine?.ssh_key?.trim()) return;
    if (getValues("ssh_key").trim() !== "") return;
    setValue("ssh_key", settings.ssh_key, { shouldValidate: false });
  }, [open, machine?.ssh_key, settings?.ssh_key, setValue, getValues]);

  // same prefill for the username: the autoinstall seed falls back to the
  // global install username anyway, so showing it up front makes the fallback
  // visible. A machine's own username always wins, and like the SSH key this
  // only runs when the drawer opens or settings load — never on keystrokes.
  useEffect(() => {
    if (!open || !settings?.install_username) return;
    if (machine?.username?.trim()) return;
    if (getValues("username").trim() !== "") return;
    setValue("username", settings.install_username, { shouldValidate: false });
  }, [open, machine?.username, settings?.install_username, setValue, getValues]);

  // same prefill for the install disk: show the global setting's disk up
  // front so the user sees what would be pinned and can overwrite it with
  // this machine's own disk. A machine's own disk always wins, and like the
  // other prefills this only runs when the drawer opens or settings load.
  useEffect(() => {
    if (!open || !settings?.install_disk) return;
    if (machine?.install_disk?.trim()) return;
    if (getValues("install_disk").trim() !== "") return;
    setValue("install_disk", settings.install_disk, { shouldValidate: false });
  }, [open, machine?.install_disk, settings?.install_disk, setValue, getValues]);

  // same prefill for the timezone: show the global setting's value up front
  // so the user sees what would be applied and can pick a per-machine one.
  // Choosing "Global default" clears the field back to the settings fallback.
  useEffect(() => {
    if (!open || !settings?.timezone) return;
    if (machine?.timezone?.trim()) return;
    if (getValues("timezone").trim() !== "") return;
    setValue("timezone", settings.timezone, { shouldValidate: false });
  }, [open, machine?.timezone, settings?.timezone, setValue, getValues]);

  // same prefill for packages: a machine without its own list starts with
  // the global defaults as editable chips instead of an invisible fallback
  useEffect(() => {
    if (!open) return;
    const defaults = parsePackageList(settings?.packages ?? "");
    if (defaults.length === 0) return;
    if (getValues("packages").length > 0) return;
    setValue("packages", defaults, { shouldValidate: false });
  }, [open, machine?.packages, settings?.packages, setValue, getValues]);

  function onSubmit(values: MachineFormValues) {
    const params = {
      name: toOptional(values.name),
      username: toOptional(values.username),
      ssh_key: toOptional(values.ssh_key),
      // an empty disk falls back to autoinstall's default (largest disk)
      install_disk: toOptional(values.install_disk),
      // a toggled-but-empty URL means "no cloud-init": the server seed is used
      cloudinit_url: useCloudinit ? toOptional(values.cloudinit_url) : null,
      notes: toOptional(values.notes),
      // an empty list falls back to the global default packages
      packages: values.packages.length > 0 ? values.packages.join("\n") : null,
      // empty timezone falls back to the global settings at install time
      timezone: toOptional(values.timezone),
      // write-only: a blank password keeps the current password source
      password: toOptional(values.password),
    };
    if (isEdit && machine) {
      updateMachine.mutate(
        {
          id: String(machine.id),
          params: {
            ...params,
            password_reset: clearPassword && !params.password ? true : null,
          },
        },
        {
          onSuccess: () => {
            toast.success(`Machine ${formatMac(machine.mac)} updated`);
            onOpenChange(false);
          },
          onError: (err) =>
            toast.error(errorMessage(err, "Failed to update machine")),
        },
      );
    } else {
      createMachine.mutate(
        { mac: values.mac.trim(), ...params },
        {
          onSuccess: () => {
            toast.success("Machine registered");
            onOpenChange(false);
          },
          onError: (err) =>
            toast.error(errorMessage(err, "Failed to register machine")),
        },
      );
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="sm:max-w-md">
        <SheetHeader>
          <SheetTitle className="flex items-center gap-2">
            {isEdit ? "Edit machine" : "Add machine"}
            {isEdit && machine && <StatusBadge status={machine.status} />}
          </SheetTitle>
          <SheetDescription>
            {isEdit ? (
              <>
                Install identity for{" "}
                <span className="text-foreground rounded-md border border-border bg-muted/50 px-1.5 py-0.5 font-mono text-xs font-semibold">
                  {formatMac(machine.mac)}
                </span>
                .{" "}
                {isInstalled
                  ? "This machine is installed — its install identity is read-only (notes stay editable). Reset the machine to edit it."
                  : "Empty fields fall back to the global settings; the disk falls back to the global install disk (or autoinstall's largest disk); the package list falls back to the default packages."}
              </>
            ) : (
              "Register a machine before its first network boot. It starts as pending."
            )}
          </SheetDescription>
        </SheetHeader>
        <form
          onSubmit={handleSubmit(onSubmit)}
          className="flex flex-1 flex-col gap-4 overflow-y-auto px-4"
        >
          {!isEdit && (
            <div className="flex flex-col gap-2">
              <Label htmlFor="machine-mac">MAC address</Label>
              <Input
                id="machine-mac"
                placeholder="aa:bb:cc:dd:ee:ff"
                className="font-mono uppercase"
                {...register("mac")}
              />
              <FieldError message={errors.mac?.message} />
            </div>
          )}

          <div className="flex flex-col gap-2">
            <Label htmlFor="machine-name">Hostname</Label>
            <Input
              id="machine-name"
              placeholder="ztp-node-1"
              readOnly={isInstalled}
              {...register("name")}
            />
            <FieldError message={errors.name?.message} />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="machine-username">Username</Label>
            <Input
              id="machine-username"
              placeholder="ubuntu"
              readOnly={isInstalled}
              {...register("username")}
            />
            <FieldError message={errors.username?.message} />
          </div>

          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <Label htmlFor="machine-password">Install password</Label>
              {isEdit && machine.password_set && !clearPassword && !isInstalled && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-6 px-2 text-xs"
                  onClick={() => {
                    setClearPassword(true);
                    setValue("password", "", { shouldValidate: false });
                  }}
                >
                  Use global password
                </Button>
              )}
            </div>
            <Input
              id="machine-password"
              type="password"
              autoComplete="new-password"
              placeholder="••••••••"
              readOnly={isInstalled}
              onInput={() => setClearPassword(false)}
              {...register("password")}
            />
            <p className="text-muted-foreground text-xs">
              {clearPassword
                ? "Will fall back to the global install password on save."
                : machine?.password_set
                  ? "Custom password set. Leave blank to keep it, type to replace."
                  : settings?.install_password_set
                    ? "Leave blank to use the global install password."
                    : "Leave blank for a locked account, or set one here for this machine."}
            </p>
            <FieldError message={errors.password?.message} />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="machine-install-disk">Install disk</Label>
            <Input
              id="machine-install-disk"
              placeholder="/dev/sda"
              className="font-mono"
              readOnly={isInstalled}
              {...register("install_disk")}
            />
            {!isInstalled && (
              <CommonDiskButtons
                onPick={(disk) =>
                  setValue("install_disk", disk, { shouldValidate: true })
                }
                disabled={isInstalled}
              />
            )}
            <FieldError message={errors.install_disk?.message} />
          </div>

          <Controller
            control={control}
            name="packages"
            render={({ field }) => (
              <div className="flex flex-col gap-2">
                <Label>Packages</Label>
                <PackagesInput
                  id="machine-packages"
                  value={field.value}
                  onChange={field.onChange}
                  disabled={isInstalled}
                />
              </div>
            )}
          />

          <Controller
            control={control}
            name="timezone"
            render={({ field }) => {
              const inList = GMT_OPTIONS.some((tz) => tz.value === field.value);
              // Base UI's <Select.Value> only renders the human label of the
              // selected item when the root gets `items`; without it the raw
              // value ("Etc/GMT-7") leaks into the trigger. Mirror the popup
              // entries here, including the two UI sentinels.
              const timezoneItems = [
                ...(field.value !== "" && !inList
                  ? [
                      {
                        value: TIMEZONE_CUSTOM,
                        label: timezoneLabel(field.value),
                      },
                    ]
                  : []),
                {
                  value: TIMEZONE_INHERIT,
                  label: `Global default${
                    settings?.timezone
                      ? ` (${timezoneLabel(settings.timezone)})`
                      : ""
                  }`,
                },
                ...GMT_OPTIONS,
              ];
              return (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="machine-timezone">Timezone</Label>
                  <Select
                    items={timezoneItems}
                    value={
                      field.value === ""
                        ? TIMEZONE_INHERIT
                        : inList
                          ? field.value
                          : TIMEZONE_CUSTOM
                    }
                    onValueChange={(value) =>
                      field.onChange(
                        value === TIMEZONE_INHERIT || value === TIMEZONE_CUSTOM
                          ? ""
                          : value,
                      )
                    }
                    disabled={isInstalled}
                  >
                    <SelectTrigger id="machine-timezone" className="w-full">
                      <SelectValue placeholder="Select timezone" />
                    </SelectTrigger>
                    <SelectContent>
                      {field.value !== "" && !inList && (
                        // a stored zone outside the GMT list (e.g. an IANA
                        // zone set by hand) renders as its own entry
                        <SelectItem value={TIMEZONE_CUSTOM}>
                          {timezoneLabel(field.value)}
                        </SelectItem>
                      )}
                      <SelectItem value={TIMEZONE_INHERIT}>
                        Global default
                        {settings?.timezone
                          ? ` (${timezoneLabel(settings.timezone)})`
                          : ""}
                      </SelectItem>
                      {GMT_OPTIONS.map((tz) => (
                        <SelectItem key={tz.value} value={tz.value}>
                          {tz.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <p className="text-muted-foreground text-xs">
                    {field.value === "" && settings?.timezone
                      ? `Empty follows the global setting (${timezoneLabel(settings.timezone)}).`
                      : "Applied to this machine's installed system only."}
                  </p>
                  <FieldError message={errors.timezone?.message} />
                </div>
              );
            }}
          />

          <div className="flex flex-col gap-2">
            <Label htmlFor="machine-ssh-key">SSH public key</Label>
            <Textarea
              id="machine-ssh-key"
              placeholder="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5... user@host"
              className="min-h-24 font-mono text-xs"
              readOnly={isInstalled}
              {...register("ssh_key")}
            />
            <FieldError message={errors.ssh_key?.message} />
          </div>

          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="machine-use-cloudinit"
                checked={useCloudinit}
                onChange={(e) => setUseCloudinit(e.target.checked)}
                disabled={isInstalled}
                className="h-4 w-4 cursor-pointer accent-primary disabled:cursor-not-allowed disabled:opacity-50"
              />
              <Label htmlFor="machine-use-cloudinit" className="cursor-pointer">
                Use custom cloud-init URL
              </Label>
            </div>
            {useCloudinit && (
              <>
                <Input
                  id="machine-cloudinit-url"
                  placeholder="https://seed.example.com/nocloud/"
                  className="font-mono text-xs"
                  readOnly={isInstalled}
                  {...register("cloudinit_url")}
                />
                <p className="text-muted-foreground text-xs">
                  Must serve user-data and meta-data (trailing slash). Left
                  empty, the server generates the seed itself.
                </p>
                <FieldError message={errors.cloudinit_url?.message} />
              </>
            )}
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="machine-notes">Notes</Label>
            <Textarea
              id="machine-notes"
              placeholder="Optional notes about this machine"
              className="min-h-20"
              {...register("notes")}
            />
            <FieldError message={errors.notes?.message} />
          </div>
        </form>
        <SheetFooter className="flex-row justify-end gap-2 border-t">
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={isPending}
          >
            Cancel
          </Button>
          <Button onClick={handleSubmit(onSubmit)} disabled={isPending}>
            {isPending
              ? "Saving…"
              : isEdit
                ? "Save changes"
                : "Add machine"}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}