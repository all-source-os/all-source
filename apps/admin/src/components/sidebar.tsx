"use client";

import { Button, Icons } from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import {
  Activity,
  Bell,
  ChevronLeft,
  ChevronRight,
  CreditCard,
  Gauge,
  HeartPulse,
  LifeBuoy,
  Mail,
  ShieldCheck,
  UserRoundSearch,
  Users,
} from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";
import { fetchHighSeverityCount } from "@/lib/suspicious-activity-api";

interface SidebarProps {
  collapsed: boolean;
  onToggle: () => void;
}

const navigation = [
  { name: "Tenants", href: "/tenants", icon: Users },
  { name: "Inbox", href: "/inbox", icon: Mail },
  { name: "Design partners", href: "/design-partners", icon: UserRoundSearch },
  { name: "Fleet", href: "/fleet", icon: HeartPulse },
  { name: "Outreach", href: "/outreach", icon: LifeBuoy },
  { name: "Efficiency", href: "/comms-efficiency", icon: Gauge },
  { name: "Monitoring", href: "/monitoring", icon: Activity },
  { name: "Billing", href: "/billing", icon: CreditCard },
  { name: "Security", href: "/security", icon: ShieldCheck },
];

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const pathname = usePathname();
  const [highAlertCount, setHighAlertCount] = useState(0);

  // Fetch high-severity alert count on mount + every 60s
  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      try {
        const count = await fetchHighSeverityCount();
        if (!cancelled) setHighAlertCount(count);
      } catch {
        // Silently ignore — badge just won't show
      }
    };

    load();
    const interval = setInterval(load, 60_000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

  return (
    <aside
      className={cn(
        "fixed left-0 top-0 z-40 flex h-screen flex-col border-r border-border bg-background transition-all duration-300",
        collapsed ? "w-16" : "w-64"
      )}
    >
      {/* Logo */}
      <div className="flex h-16 items-center justify-between border-b border-border px-4">
        <Link href="/tenants" className="group flex items-center gap-2">
          <Icons.logo className="h-8 w-8 shrink-0 text-primary transition-transform duration-300 group-hover:scale-110 group-hover:rotate-12" />
          {!collapsed && (
            <div className="flex flex-col">
              <span className="text-lg font-semibold tracking-tight bg-gradient-to-r from-foreground to-foreground bg-clip-text transition-all duration-300 group-hover:from-primary group-hover:to-foreground group-hover:text-transparent">
                AllSource
              </span>
              <span className="text-[10px] font-medium uppercase tracking-widest text-muted-foreground">
                Admin
              </span>
            </div>
          )}
        </Link>
        <Button
          variant="ghost"
          size="icon"
          onClick={onToggle}
          className="h-8 w-8 shrink-0"
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronLeft className="h-4 w-4" />}
        </Button>
      </div>

      {/* Navigation */}
      <nav className="flex-1 space-y-1 overflow-y-auto px-2 py-4">
        {navigation.map((item) => {
          const isActive = pathname.startsWith(item.href);

          return (
            <Link
              key={item.name}
              href={item.href}
              className={cn(
                "flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors",
                isActive
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
                collapsed && "justify-center px-2"
              )}
              title={collapsed ? item.name : undefined}
            >
              <item.icon className="h-5 w-5 shrink-0" />
              {!collapsed && <span>{item.name}</span>}
            </Link>
          );
        })}

        {/* Suspicious Activity link with badge */}
        <Link
          href="/security/alerts"
          className={cn(
            "flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors relative",
            pathname.startsWith("/security/alerts")
              ? "bg-primary/10 text-primary"
              : "text-muted-foreground hover:bg-muted hover:text-foreground",
            collapsed && "justify-center px-2"
          )}
          title={collapsed ? "Alerts" : undefined}
          data-testid="sidebar-alerts-link"
        >
          <div className="relative shrink-0">
            <Bell className="h-5 w-5" />
            {highAlertCount > 0 && (
              <span
                className="absolute -right-1.5 -top-1.5 flex h-4 min-w-[16px] items-center justify-center rounded-full bg-red-500 px-1 text-[10px] font-bold text-white"
                data-testid="sidebar-alert-badge"
              >
                {highAlertCount > 99 ? "99+" : highAlertCount}
              </span>
            )}
          </div>
          {!collapsed && <span>Alerts</span>}
        </Link>
      </nav>

      {/* Status indicator */}
      {collapsed && (
        <div className="border-t border-border p-2">
          <div className="flex flex-col items-center gap-2 py-2">
            <div className="h-2 w-2 animate-pulse rounded-full bg-green-500" />
          </div>
        </div>
      )}
    </aside>
  );
}
