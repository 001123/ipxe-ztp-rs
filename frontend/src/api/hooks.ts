import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { ApproveMachineParams } from "@/bindings/ApproveMachineParams";
import type { CreateMachineParams } from "@/bindings/CreateMachineParams";
import type { CreateOsVersionParams } from "@/bindings/CreateOsVersionParams";
import type { MachineResponse } from "@/bindings/MachineResponse";
import type { OsVersionResponse } from "@/bindings/OsVersionResponse";
import type { Page } from "@/bindings/Page";
import type { SettingsResponse } from "@/bindings/SettingsResponse";
import type { UpdateMachineParams } from "@/bindings/UpdateMachineParams";
import type { UpdateOsVersionParams } from "@/bindings/UpdateOsVersionParams";
import type { UpdateSettingsParams } from "@/bindings/UpdateSettingsParams";
import { del, get, post, put } from "./client";

/** Live status for machines: poll so state changes show up without a reload. */
const MACHINES_REFETCH_MS = 5000;

function isOsDownloadInFlight(os: OsVersionResponse): boolean {
  return os.download_status === "pending" || os.download_status === "downloading";
}

// Machines ----------------------------------------------------------------—

export function machinesListPath(status?: string): string {
  return status ? `/api/machines?status=${encodeURIComponent(status)}` : "/api/machines";
}

export function useMachines(status?: string) {
  return useQuery({
    queryKey: ["machines", status ?? "all"],
    queryFn: () => get<Page<MachineResponse>>(machinesListPath(status)),
    refetchInterval: MACHINES_REFETCH_MS,
  });
}

export function useMachine(id: string) {
  return useQuery({
    queryKey: ["machines", "detail", id],
    queryFn: () => get<MachineResponse>(`/api/machines/${id}`),
  });
}

export function useApproveMachine() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, osVersionId }: { id: string; osVersionId: number }) => {
      // ts-rs types ids as bigint, but JSON never carries real bigints —
      // the runtime value is a number, hence the cast.
      const body = { os_version_id: osVersionId } as unknown as ApproveMachineParams;
      return post<unknown>(`/api/machines/${id}/approve`, body);
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["machines"] }),
  });
}

export function useResetMachine() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => post<unknown>(`/api/machines/${id}/reset`, {}),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["machines"] }),
  });
}

export function useDeleteMachine() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/api/machines/${id}`),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["machines"] }),
  });
}

export function useCreateMachine() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: CreateMachineParams) => post<unknown>("/api/machines", params),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["machines"] }),
  });
}

export function useUpdateMachine() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, params }: { id: string; params: UpdateMachineParams }) =>
      put<MachineResponse>(`/api/machines/${id}`, params),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["machines"] }),
  });
}

// OS versions ---------------------------------------------------------------

export function useOsVersions() {
  return useQuery({
    queryKey: ["os-versions"],
    queryFn: () => get<Page<OsVersionResponse>>("/api/os_versions"),
    // Poll faster while any mirror download is running.
    refetchInterval: (query) => {
      const items = query.state.data?.items ?? [];
      return items.some(isOsDownloadInFlight) ? 2000 : false;
    },
  });
}

export function useCreateOsVersion() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: CreateOsVersionParams) =>
      post<unknown>("/api/os_versions", params),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["os-versions"] }),
  });
}

export function useUpdateOsVersion() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, params }: { id: string; params: UpdateOsVersionParams }) =>
      put<unknown>(`/api/os_versions/${id}`, params),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["os-versions"] }),
  });
}

export function useDeleteOsVersion() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => del(`/api/os_versions/${id}`),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["os-versions"] }),
  });
}

export function useDownloadOsVersion() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      post<unknown>(`/api/os_versions/${id}/download`, {}),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["os-versions"] }),
  });
}

// Settings ------------------------------------------------------------------

export function useSettings() {
  return useQuery({
    queryKey: ["settings"],
    queryFn: () => get<SettingsResponse>("/api/settings"),
  });
}

export function useUpdateSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: UpdateSettingsParams) =>
      put<unknown>("/api/settings", params),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["settings"] }),
  });
}
