import { useEffect, useState } from "react";
import { Camera, History as HistoryIcon, BookOpen, Settings } from "lucide-react";
import { THEMES, ThemeName, loadThemeFromStore } from "../utils/theme";
import CaptureHome from "./CaptureHome";
import History from "./History";
import SnippetsPage from "./SnippetsPage";
import SettingsModal from "./SettingsModal";

export type MainTab = "capture" | "history" | "snippets";

interface AppShellProps {
  tab: MainTab;
  settingsOpen: boolean;
  onNavigate: (tab: MainTab) => void;
  onOpenSettings: () => void;
  onCloseSettings: () => void;
}

const NAV_ITEMS: { tab: MainTab; label: string; icon: typeof Camera }[] = [
  { tab: "capture", label: "Capture", icon: Camera },
  { tab: "history", label: "History", icon: HistoryIcon },
  { tab: "snippets", label: "Snippets", icon: BookOpen },
];

export default function AppShell({ tab, settingsOpen, onNavigate, onOpenSettings, onCloseSettings }: AppShellProps) {
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");

  useEffect(() => {
    loadThemeFromStore().then(setCurrentTheme);
  }, []);

  const theme = THEMES[currentTheme];

  return (
    <div
      className="h-screen w-full flex overflow-hidden"
      style={{
        backgroundColor: theme.canvasBg,
        color: theme.textColor,
        fontFamily: theme.fontFamily,
      }}
    >
      {/* Sidebar */}
      <div
        className="shrink-0 w-52 flex flex-col border-r"
        style={{
          backgroundColor: theme.toolbar,
          borderColor: theme.toolbarBorder,
        }}
      >
        {/* App identity */}
        <div className="px-4 py-4 flex items-center gap-2.5 border-b" style={{ borderColor: theme.toolbarBorder }}>
          <Camera size={20} style={{ color: theme.accentColor }} />
          <span className="text-base font-bold">Madera.SS</span>
        </div>

        {/* Main navigation */}
        <nav className="flex-1 px-2 py-3 flex flex-col gap-1">
          {NAV_ITEMS.map(({ tab: itemTab, label, icon: Icon }) => {
            const active = tab === itemTab && !settingsOpen;
            return (
              <button
                key={itemTab}
                onClick={() => onNavigate(itemTab)}
                className="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors text-left"
                style={{
                  backgroundColor: active ? theme.buttonActive : "transparent",
                  color: active
                    ? (theme.name === "Candy Pop" ? "#fff" : theme.canvasBg)
                    : theme.textColor,
                  opacity: active ? 1 : 0.75,
                }}
              >
                <Icon size={17} />
                {label}
              </button>
            );
          })}
        </nav>

        {/* Settings pinned at bottom */}
        <div className="px-2 py-3 border-t" style={{ borderColor: theme.toolbarBorder }}>
          <button
            onClick={onOpenSettings}
            className="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors text-left"
            style={{
              backgroundColor: settingsOpen ? theme.buttonActive : "transparent",
              color: settingsOpen
                ? (theme.name === "Candy Pop" ? "#fff" : theme.canvasBg)
                : theme.textColor,
              opacity: settingsOpen ? 1 : 0.75,
            }}
          >
            <Settings size={17} />
            Settings
          </button>
        </div>
      </div>

      {/* Main content */}
      <div className="flex-1 min-w-0 min-h-0 flex flex-col">
        {tab === "capture" && <CaptureHome onNavigate={onNavigate} />}
        {tab === "history" && <History />}
        {tab === "snippets" && <SnippetsPage />}
      </div>

      {/* Settings as overlay, so closing returns exactly where you were */}
      <SettingsModal isOpen={settingsOpen} onClose={onCloseSettings} />
    </div>
  );
}
