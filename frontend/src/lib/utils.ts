export { cn } from "cn"

/** Display MACs uppercase (aa:bb:… → AA:BB:…); stored values stay untouched. */
export function formatMac(mac: string | null | undefined): string {
  return mac ? mac.toUpperCase() : "—";
}
