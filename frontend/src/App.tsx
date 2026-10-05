import { useState } from "react";
import type { CSSProperties } from "react";
import { Outlet } from "react-router"
import { getToken } from "./auth/token"
import { AppHeader } from "./components/layout/AppHeader"
import { AppSidebar } from "./components/layout/AppSidebar"
import { Toaster } from "./components/ui/sonner"
import { SidebarInset, SidebarProvider } from "./components/ui/sidebar"
import { TooltipProvider } from "./components/ui/tooltip"
import { useTheme } from "./hooks/use-theme"

const SIDEBAR_OPEN_KEY = "sidebar_open";

/** Toaster themed by our own use-theme hook (night renders as dark). */
function AppToaster() {
  const { resolvedTheme } = useTheme()
  return <Toaster theme={resolvedTheme} position="top-center" />
}

function readSidebarOpen(): boolean {
  try {
    return window.localStorage.getItem(SIDEBAR_OPEN_KEY) !== "false";
  } catch {
    return true;
  }
}

export function App() {
  const isAuthenticated = getToken() !== null
  // Single SidebarProvider for the whole shell: AppSidebar must be a child of
  // this provider, not wrap its own (nested providers break the flex layout).
  const [sidebarOpen, setSidebarOpen] = useState(readSidebarOpen);

  function handleSidebarOpenChange(next: boolean) {
    setSidebarOpen(next);
    try {
      window.localStorage.setItem(SIDEBAR_OPEN_KEY, String(next));
    } catch {
      // persistence is best-effort
    }
  }

  // Auth pages (login) render alone: no layout shell above them.
  if (!isAuthenticated) {
    return (
      <>
        <Outlet />
        <AppToaster />
      </>
    )
  }

  return (
    <TooltipProvider>
      <SidebarProvider
        open={sidebarOpen}
        onOpenChange={handleSidebarOpenChange}
        style={{ "--sidebar-width-icon": "3.5rem" } as CSSProperties}
      >
        <AppSidebar />
        <SidebarInset>
          <AppHeader />
          <main className="flex-1 overflow-auto">
            {/* full-width: the sidebar already reserves the left edge, and
                narrow pages (Settings) cap themselves with max-w-2xl cards */}
            <div className="w-full space-y-6 p-6">
              <Outlet />
            </div>
          </main>
        </SidebarInset>
        <AppToaster />
      </SidebarProvider>
    </TooltipProvider>
  )
}