import { Controller, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { Loader2, Save } from "lucide-react";
import { toast } from "sonner";
import { useOsVersions, useSettings, useUpdateSettings } from "@/api/hooks";
import { ApiClientError } from "@/api/client";
import { PackagesInput, parsePackageList } from "@/components/PackagesInput";
import { CommonDiskButtons } from "@/components/CommonDiskButtons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { GMT_OPTIONS } from "@/lib/timezones";

const settingsSchema = z.object({
  default_os_version_id: z.number().nullable(),
  install_username: z.string().min(1, "Install username is required"),
  install_password: z.string(),
  ssh_key: z.string(),
  install_disk: z
    .string()
    .trim()
    .max(64, "Disk path is too long.")
    .refine(
      (value) => value === "" || /^\/dev\/[A-Za-z0-9./_-]+$/.test(value),
      "Disk must be a device path like /dev/sda.",
    ),
  timezone: z.string().min(1, "Timezone is required"),
  public_host: z.string(),
  nfs_host: z.string(),
  nfs_export_root: z.string(),
  packages: z.array(z.string()).max(100, "At most 100 packages are allowed."),
});

type SettingsFormValues = z.infer<typeof settingsSchema>;

function osVersionLabel(os: { name: string; version: string; arch: string }) {
  return `${os.name} ${os.version} (${os.arch})`;
}

export function Settings() {
  const { data: settings, isLoading } = useSettings();
  const { data: osData } = useOsVersions();
  const updateSettings = useUpdateSettings();

  const osVersions = osData?.items ?? [];

  const form = useForm<SettingsFormValues>({
    resolver: zodResolver(settingsSchema),
    // defaults keep controlled fields (the packages Controller) defined on
    // the first mount, before `values` below has been applied
    defaultValues: {
      default_os_version_id: null,
      install_username: "ubuntu",
      // write-only: never prefilled, reset on every refetch
      install_password: "",
      ssh_key: "",
      timezone: "UTC",
      public_host: "",
      nfs_host: "",
      nfs_export_root: "",
      packages: [],
      install_disk: "",
    },
    // `values` (not defaultValues) keeps the form in sync while the settings
    // query is loading / refetching.
    values: settings
      ? {
          default_os_version_id: settings.default_os_version_id,
          install_username: settings.install_username,
          // write-only: never prefilled, reset on every refetch
          install_password: "",
          ssh_key: settings.ssh_key,
          timezone: settings.timezone,
          public_host: settings.public_host ?? "",
          nfs_host: settings.nfs_host ?? "",
          nfs_export_root: settings.nfs_export_root ?? "",
          packages: parsePackageList(settings.packages),
          install_disk: settings.install_disk ?? "",
        }
      : undefined,
  });

  function handleSubmit(values: SettingsFormValues) {
    // Settings are a key/value map on the wire; null → empty string clears.
    updateSettings.mutate(
      {
        values: {
          default_os_version_id:
            values.default_os_version_id === null
              ? ""
              : String(values.default_os_version_id),
          install_username: values.install_username,
          install_password: values.install_password,
          ssh_key: values.ssh_key,
          timezone: values.timezone,
          public_host: values.public_host,
          nfs_host: values.nfs_host,
          nfs_export_root: values.nfs_export_root,
          packages: values.packages.join("\n"),
          install_disk: values.install_disk,
        },
      },
      {
        onSuccess: () => toast.success("Settings saved"),
        onError: (err) =>
          toast.error(
            err instanceof ApiClientError ? err.message : "Failed to save settings",
          ),
      },
    );
  }

  const errors = form.formState.errors;

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        <p className="text-muted-foreground text-sm">
          Defaults used when rendering autoinstall configurations.
        </p>
      </div>

      {isLoading ? (
        <Card className="max-w-2xl">
          <CardContent className="space-y-4">
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className="h-9 w-full" />
            ))}
          </CardContent>
        </Card>
      ) : (
        <Card className="max-w-2xl">
          <CardHeader>
            <CardTitle>Provisioning</CardTitle>
            <CardDescription>
              Applied to every autoinstall rendered by the boot server.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form
              onSubmit={form.handleSubmit(handleSubmit)}
              className="space-y-4"
              noValidate
            >
              <div className="grid gap-2">
                <Label htmlFor="default_os_version_id">Default OS version</Label>
                <Controller
                  control={form.control}
                  name="default_os_version_id"
                  render={({ field }) => (
                    <Select
                      items={osVersions.map((os) => ({
                        value: os.id,
                        label: osVersionLabel(os),
                      }))}
                      value={field.value}
                      onValueChange={(value) => field.onChange(value)}
                    >
                      <SelectTrigger
                        id="default_os_version_id"
                        aria-label="Default OS version"
                        className="w-full"
                      >
                        <SelectValue placeholder="Select default OS" />
                      </SelectTrigger>
                      <SelectContent>
                        {osVersions.map((os) => (
                          <SelectItem key={String(os.id)} value={os.id}>
                            {osVersionLabel(os)}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                />
                {errors.default_os_version_id && (
                  <p role="alert" className="text-destructive text-sm">
                    {errors.default_os_version_id.message}
                  </p>
                )}
              </div>

              <div className="grid gap-2">
                <Label htmlFor="install_username">Install username</Label>
                <Input
                  id="install_username"
                  placeholder="ubuntu"
                  aria-invalid={Boolean(errors.install_username)}
                  {...form.register("install_username")}
                />
                {errors.install_username && (
                  <p role="alert" className="text-destructive text-sm">
                    {errors.install_username.message}
                  </p>
                )}
              </div>

              <div className="grid gap-2">
                <div className="flex items-center justify-between">
                  <Label htmlFor="install_password">Install password</Label>
                  {settings?.install_password_set ? (
                    <Badge variant="secondary">Password is set</Badge>
                  ) : (
                    <Badge variant="outline">No password set</Badge>
                  )}
                </div>
                <Input
                  id="install_password"
                  type="password"
                  autoComplete="new-password"
                  placeholder={
                    settings?.install_password_set
                      ? "Leave blank to keep the current password"
                      : "No password set — enter one"
                  }
                  {...form.register("install_password")}
                />
                <p className="text-muted-foreground text-xs">
                  {settings?.install_password_set
                    ? "Update password for the install user on provisioned machines. Leave blank to keep it, or type a new one and save."
                    : "Login password for the install user on provisioned machines. Stored as a SHA-512 crypt hash."}
                </p>
              </div>

              <div className="grid gap-2">
                <Label htmlFor="install_disk">Install disk</Label>
                <Input
                  id="install_disk"
                  placeholder="/dev/sda"
                  className="font-mono"
                  aria-invalid={Boolean(errors.install_disk)}
                  {...form.register("install_disk")}
                />
                <CommonDiskButtons
                  onPick={(disk) =>
                    form.setValue("install_disk", disk, { shouldValidate: true })
                  }
                />
                {errors.install_disk && (
                  <p role="alert" className="text-destructive text-sm">
                    {errors.install_disk.message}
                  </p>
                )}
                <p className="text-muted-foreground text-xs">
                  Disk pinned in every rendered autoinstall. A machine&apos;s
                  own install disk overrides this; empty lets autoinstall pick
                  the largest disk.
                </p>
              </div>

              <div className="grid gap-2">
                <Label htmlFor="ssh_key">SSH key</Label>
                <Textarea
                  id="ssh_key"
                  placeholder="ssh-ed25519 AAAA…"
                  className="font-mono text-xs"
                  rows={4}
                  {...form.register("ssh_key")}
                />
                <p className="text-muted-foreground text-xs">
                  Authorized key injected into provisioned machines.
                </p>
              </div>

              <Controller
                control={form.control}
                name="packages"
                render={({ field }) => (
                  <div className="grid gap-2">
                    <Label>Default packages</Label>
                    <PackagesInput id="settings-packages" value={field.value} onChange={field.onChange} />
                    <p className="text-muted-foreground text-xs">
                      Installed on every provisioned machine right after the OS
                      install. A machine&apos;s own package list overrides this.
                    </p>
                  </div>
                )}
              />

              <div className="grid gap-2">
                <Label htmlFor="timezone">Timezone</Label>
                <Controller
                  control={form.control}
                  name="timezone"
                  render={({ field }) => (
                    <Select
                      items={GMT_OPTIONS}
                      value={field.value}
                      onValueChange={(value) => field.onChange(value)}
                    >
                      <SelectTrigger
                        id="timezone"
                        aria-label="Timezone"
                        className="w-full"
                      >
                        <SelectValue placeholder="Select timezone" />
                      </SelectTrigger>
                      <SelectContent>
                        {GMT_OPTIONS.map((tz) => (
                          <SelectItem key={tz.value} value={tz.value}>
                            {tz.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                />
                {errors.timezone && (
                  <p role="alert" className="text-destructive text-sm">
                    {errors.timezone.message}
                  </p>
                )}
                <p className="text-muted-foreground text-xs">
                  Fixed GMT offset applied to installed machines.
                </p>
              </div>

              <div className="grid gap-2">
                <Label htmlFor="public_host">Public host</Label>
                <Input
                  id="public_host"
                  placeholder="http://192.168.1.10:5150"
                  {...form.register("public_host")}
                />
                <p className="text-muted-foreground text-xs">
                  Base URL machines use to fetch the boot script and
                  autoinstall files.
                </p>
              </div>

              <div className="grid gap-2">
                <Label htmlFor="nfs_host">NFS host</Label>
                <Input
                  id="nfs_host"
                  placeholder="192.168.1.10"
                  {...form.register("nfs_host")}
                />
                <p className="text-muted-foreground text-xs">
                  Hostname/IP clients use for NFS boot mode. Defaults to the
                  public host&apos;s address.
                </p>
              </div>

              <div className="grid gap-2">
                <Label htmlFor="nfs_export_root">NFS export root</Label>
                <Input
                  id="nfs_export_root"
                  placeholder="/absolute/path/to/data"
                  {...form.register("nfs_export_root")}
                />
                <p className="text-muted-foreground text-xs">
                  Absolute path exported over NFS in place of the data dir
                  (must contain <code>os/&lt;id&gt;/casper/</code>). Defaults
                  to the server&apos;s data dir. See the README NFS setup.
                </p>
              </div>

              <div className="flex justify-end">
                <Button type="submit" disabled={updateSettings.isPending}>
                  {updateSettings.isPending ? (
                    <Loader2 data-icon="inline-start" className="animate-spin" />
                  ) : (
                    <Save data-icon="inline-start" />
                  )}
                  {updateSettings.isPending ? "Saving…" : "Save settings"}
                </Button>
              </div>
            </form>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
