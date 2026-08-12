import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import AppShell, { MainTab } from "./components/AppShell";
import SelectionOverlay from "./components/SelectionOverlay";
import Editor from "./components/Editor";
import ColorPicker from "./components/ColorPicker";
import QuickPasteModal from "./components/QuickPasteModal";
import PinView from "./components/PinView";

// Views that live in their own dedicated window (overlays, workspaces, popups)
type StandaloneView = "selection" | "editor" | "colorpicker" | "quickpaste" | "pin";

interface RouteState {
  standalone: StandaloneView | null;
  tab: MainTab;
  settingsOpen: boolean;
}

function routeFrom(text: string): RouteState | null {
  const t = text.toLowerCase();
  if (t.includes("pin")) {
    return { standalone: "pin", tab: "capture", settingsOpen: false };
  }
  if (t.includes("multipaste") || t.includes("quickpaste")) {
    return { standalone: "quickpaste", tab: "capture", settingsOpen: false };
  }
  if (t.includes("editor")) {
    return { standalone: "editor", tab: "capture", settingsOpen: false };
  }
  if (t.includes("colorpicker")) {
    return { standalone: "colorpicker", tab: "capture", settingsOpen: false };
  }
  if (t.includes("selection")) {
    return { standalone: "selection", tab: "capture", settingsOpen: false };
  }
  if (t.includes("history")) {
    return { standalone: null, tab: "history", settingsOpen: false };
  }
  if (t.includes("snippets")) {
    return { standalone: null, tab: "snippets", settingsOpen: false };
  }
  if (t.includes("settings")) {
    // Settings is an overlay: keep the underlying tab untouched
    return { standalone: null, tab: "capture", settingsOpen: true };
  }
  if (t.includes("dashboard") || t.includes("capture")) {
    return { standalone: null, tab: "capture", settingsOpen: false };
  }
  return null;
}

function currentRoute(): RouteState {
  const fromHash = routeFrom(window.location.hash.replace(/^#\/?/, "").split("?")[0]);
  if (fromHash) return fromHash;
  const fromPath = routeFrom(window.location.pathname);
  if (fromPath) return fromPath;
  return { standalone: null, tab: "capture", settingsOpen: false };
}

function App() {
  const [route, setRoute] = useState<RouteState>(currentRoute);

  useEffect(() => {
    const applyRoute = () => {
      const next = currentRoute();
      setRoute((prev) => {
        if (next.standalone) return next;
        if (next.settingsOpen) {
          // Opening settings never loses the tab you were on
          return { ...prev, standalone: null, settingsOpen: true };
        }
        return { standalone: null, tab: next.tab, settingsOpen: false };
      });
    };

    applyRoute();
    window.addEventListener("hashchange", applyRoute);

    // Navigation requests from the backend (tray, global shortcuts, CLI)
    const unlisten = listen<string>("app-navigate", (event) => {
      const target = event.payload;
      if (["capture", "history", "snippets", "settings"].includes(target)) {
        const newHash = "#/" + target;
        if (window.location.hash === newHash) {
          applyRoute();
        } else {
          window.location.hash = newHash;
        }
      }
    });

    return () => {
      window.removeEventListener("hashchange", applyRoute);
      unlisten.then((fn) => fn());
    };
  }, []);

  // Dedicated windows render their component alone
  if (route.standalone === "selection") return <SelectionOverlay />;
  if (route.standalone === "editor") return <Editor />;
  if (route.standalone === "colorpicker") return <ColorPicker />;
  if (route.standalone === "quickpaste") return <QuickPasteModal />;
  if (route.standalone === "pin") return <PinView />;

  // The main window: one shell, navigable sections
  return (
    <AppShell
      tab={route.tab}
      settingsOpen={route.settingsOpen}
      onNavigate={(tab) => {
        window.location.hash = "#/" + tab;
      }}
      onOpenSettings={() => {
        window.location.hash = "#/settings";
      }}
      onCloseSettings={() => {
        window.location.hash = "#/" + route.tab;
      }}
    />
  );
}

export default App;
