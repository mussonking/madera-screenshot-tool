import { useEffect, useState } from "react";
import Dashboard from "./components/Dashboard";
import SelectionOverlay from "./components/SelectionOverlay";
import Editor from "./components/Editor";
import History from "./components/History";
import ColorPicker from "./components/ColorPicker";
import DesktopGuardian from "./components/DesktopGuardian";
import WakePopup from "./components/WakePopup";

type View = "dashboard" | "selection" | "editor" | "history" | "colorpicker" | "desktop-guardian";

function App() {
  const [view, setView] = useState<View>("dashboard");

  useEffect(() => {
    // Determine which view to show based on URL path or hash
    const path = window.location.pathname;
    const hash = window.location.hash;

    if (hash.includes("desktop-guardian") || path.includes("desktop-guardian")) {
      setView("desktop-guardian");
    } else if (path.includes("editor")) {
      setView("editor");
    } else if (path.includes("history")) {
      setView("history");
    } else if (path.includes("colorpicker")) {
      setView("colorpicker");
    } else if (path.includes("selection")) {
      setView("selection");
    } else {
      setView("dashboard");
    }

    // Listen for hash changes (for navigation from other windows)
    const handleHashChange = () => {
      if (window.location.hash.includes("desktop-guardian")) {
        setView("desktop-guardian");
      }
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
      {/* Wake popup shown over everything when system wakes from sleep */}
      <WakePopup />
    </div>
  );
}

export default App;
