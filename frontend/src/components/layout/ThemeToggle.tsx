import { Monitor, Moon, MoonStar, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useTheme, type ThemeMode } from "@/hooks/use-theme";

const NEXT_MODE_LABEL: Record<ThemeMode, string> = {
  system: "light",
  light: "dark",
  dark: "night",
  night: "system",
};

/** Single icon button cycling system → light → dark → night; shows the next mode. */
export function ThemeToggle() {
  const { theme, cycleTheme } = useTheme();
  const next = NEXT_MODE_LABEL[theme];

  return (
    <Button
      variant="ghost"
      size="icon"
      title={`Switch to ${next} mode`}
      aria-label={`Switch to ${next} mode`}
      onClick={cycleTheme}
    >
      {theme === "system" && <Monitor />}
      {theme === "light" && <Sun />}
      {theme === "dark" && <Moon />}
      {theme === "night" && <MoonStar />}
    </Button>
  );
}