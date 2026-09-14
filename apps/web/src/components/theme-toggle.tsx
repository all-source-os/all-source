"use client";

import { Button } from "@allsource/ui";
import { Moon, Sun } from "lucide-react";
import { useTheme } from "next-themes";
import { useEffect, useState } from "react";

export function ThemeToggle() {
  const { setTheme, resolvedTheme } = useTheme();
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);
  }, []);

  if (!mounted) {
    return (
      <Button
        variant="ghost"
        size="icon"
        className="h-12 w-12 shrink-0 rounded-md border bg-background"
        aria-label="Loading theme control"
        disabled
      >
        <span className="h-5 w-5" />
      </Button>
    );
  }

  const isDark = resolvedTheme === "dark";

  return (
    <Button
      variant="ghost"
      size="icon"
      className="h-12 w-12 shrink-0 rounded-md border bg-background transition-colors"
      onClick={() => setTheme(isDark ? "light" : "dark")}
    >
      <Sun
        className={`h-5 w-5 text-primary transition-all duration-300 ${isDark ? "scale-0 rotate-90 opacity-0" : "scale-100 rotate-0 opacity-100"} absolute`}
      />
      <Moon
        className={`h-5 w-5 text-primary transition-all duration-300 ${isDark ? "scale-100 rotate-0 opacity-100" : "scale-0 -rotate-90 opacity-0"} absolute`}
      />
      <span className="sr-only">Use {isDark ? "light" : "dark"} theme</span>
    </Button>
  );
}
