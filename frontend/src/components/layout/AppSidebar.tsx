import { Link, useLocation } from "react-router";
import { Disc3, HardDrive, Network, Settings } from "lucide-react";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";

const NAV_ITEMS = [
  { title: "Machines", href: "/machines", icon: HardDrive },
  { title: "OS Versions", href: "/os-versions", icon: Disc3 },
  { title: "Settings", href: "/settings", icon: Settings },
] as const;

export function AppSidebar() {
  const location = useLocation();

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <div className="flex items-center gap-2 px-2 py-1.5">
          <Network className="size-5 shrink-0 text-sidebar-foreground" />
          <span className="text-sm font-semibold tracking-tight group-data-[collapsible=icon]:hidden">
            iPXE ZTP
          </span>
        </div>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>Admin</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {NAV_ITEMS.map((item) => (
                <SidebarMenuItem key={item.href}>
                  <SidebarMenuButton
                    render={<Link to={item.href} />}
                    isActive={location.pathname === item.href}
                    tooltip={item.title}
                  >
                    <item.icon />
                    <span>{item.title}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        <p className="text-muted-foreground truncate px-2 py-1 text-xs group-data-[collapsible=icon]:hidden">
          v0.1.0
        </p>
      </SidebarFooter>
    </Sidebar>
  );
}