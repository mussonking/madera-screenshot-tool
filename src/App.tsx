import { useEffect, useState } from "react";
import Dashboard from "./components/Dashboard";
import SelectionOverlay from "./components/SelectionOverlay";
import Editor from "./components/Editor";
import History from "./components/History";
import ColorPicker from "./components/ColorPicker";
import DesktopGuardian from "./components/DesktopGuardian";
import WakePopup from "./components/WakePopup";
import SettingsPage from "./components/SettingsPage";
import QuickPasteModal from "./components/QuickPasteModal";

type View = "dashboard" | "selection" | "editor" | "history" | "colorpicker" | "desktop-guardian" | "settings" | "quickpaste";

function App() {
  const [view, setView] = useState<View>("dashboard");

  // Auto-save is handled in the Rust backend (runs even without windows open)

  useEffect(() => {
    // Determine which view to show based on URL path or hash
    const determineView = () => {
      const path = window.location.pathname;
      const hash = window.location.hash;

      if (hash.includes("multipaste") || path.includes("multipaste") || hash.includes("quickpaste") || path.includes("quickpaste")) {
        return "quickpaste";
      } else if (hash.includes("settings") || path.includes("settings")) {
        return "settings";
      } else if (hash.includes("desktop-guardian") || path.includes("desktop-guardian")) {
        return "desktop-guardian";
      } else if (hash.includes("editor") || path.includes("editor")) {
        return "editor";
      } else if (hash.includes("history") || path.includes("history")) {
        return "history";
      } else if (hash.includes("colorpicker") || path.includes("colorpicker")) {
        return "colorpicker";
      } else if (hash.includes("selection") || path.includes("selection")) {
        return "selection";
      } else if (hash.includes("dashboard") || path.includes("dashboard")) {
        return "dashboard";
      } else {
        return "dashboard";
      }
    };

    setView(determineView());

    // Listen for hash changes (for navigation from other windows)
    const handleHashChange = () => {
      setView(determineView());
    };
    window.addEventListener("hashchange", handleHashChange);
    return () => window.removeEventListener("hashchange", handleHashChange);
  }, []);

  return (
    <div className="h-full w-full">
      {view === "dashboard" && <Dashboard />}
      {view === "selection" && <SelectionOverlay />}
      {view === "editor" && <Editor />}
      {view === "history" && <History />}
      {view === "colorpicker" && <ColorPicker />}
      {view === "desktop-guardian" && <DesktopGuardian />}
      {view === "quickpaste" && <QuickPasteModal />}
      {view === "settings" && <SettingsPage />}
      {/* Wake popup shown over everything when system wakes from sleep */}
      <WakePopup />
    </div>
  );
}

export default App;
