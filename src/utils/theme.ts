import { load } from "@tauri-apps/plugin-store";

export type ThemeName = "default" | "cyberpunk" | "retro" | "candy" | "sketch" | "neon" | "midnight" | "obsidian" | "nord" | "dracula" | "monokai";

export interface Theme {
    name: string;
    toolbar: string;
    toolbarBorder: string;
    buttonBg: string;
    buttonActive: string;
    canvasBg: string;
    textColor: string;
    accentColor: string;
    fontFamily: string;
    borderRadius: string;
    borderStyle: string;
}

export const THEMES: Record<ThemeName, Theme> = {
    default: {
        name: "Default",
        toolbar: "#16213e",
        toolbarBorder: "#0f3460",
        buttonBg: "#0f3460",
        buttonActive: "#e94560",
        canvasBg: "#1a1a2e",
        textColor: "#e5e5e5",
        accentColor: "#e94560",
        fontFamily: "'Segoe UI', system-ui, sans-serif",
        borderRadius: "8px",
        borderStyle: "solid",
    },
    cyberpunk: {
        name: "Cyberpunk 2077",
        toolbar: "#0c0c0c",
        toolbarBorder: "#fcee0a",
        buttonBg: "#1a1a1a",
        buttonActive: "#fcee0a",
        canvasBg: "#0a0a0a",
        textColor: "#fcee0a",
        accentColor: "#00f0ff",
        fontFamily: "'Orbitron', 'Share Tech Mono', monospace",
        borderRadius: "0px",
        borderStyle: "solid",
    },
    retro: {
        name: "Terminal",
        toolbar: "#0a140a",
        toolbarBorder: "#1a3a1a",
        buttonBg: "#0d1f0d",
        buttonActive: "#33bb33",
        canvasBg: "#050d05",
        textColor: "#33bb33",
        accentColor: "#22aa22",
        fontFamily: "'VT323', 'Courier New', monospace",
        borderRadius: "0px",
        borderStyle: "solid",
    },
    candy: {
        name: "Candy Pop",
        toolbar: "#fff0f5",
        toolbarBorder: "#ffb6c1",
        buttonBg: "#ffe4ec",
        buttonActive: "#ff6b9d",
        canvasBg: "#fff5f8",
        textColor: "#c44569",
        accentColor: "#ff6b9d",
        fontFamily: "'Comic Sans MS', cursive",
        borderRadius: "20px",
        borderStyle: "solid",
    },
    sketch: {
        name: "Sketch",
        toolbar: "#fefefe",
        toolbarBorder: "#ccc",
        buttonBg: "#f0f0f0",
        buttonActive: "#2d3436",
        canvasBg: "#f8f8f8",
        textColor: "#2d3436",
        accentColor: "#0984e3",
        fontFamily: "'Segoe Print', cursive",
        borderRadius: "4px",
        borderStyle: "dashed",
    },
    neon: {
        name: "Neon Glow",
        toolbar: "#0a0015",
        toolbarBorder: "#ff00ff",
        buttonBg: "#150025",
        buttonActive: "#ff00ff",
        canvasBg: "#050010",
        textColor: "#ff88ff",
        accentColor: "#00ffff",
        fontFamily: "'Audiowide', sans-serif",
        borderRadius: "12px",
        borderStyle: "solid",
    },
    midnight: {
        name: "Midnight",
        toolbar: "#141b2d",
        toolbarBorder: "#1e2a45",
        buttonBg: "#1a2540",
        buttonActive: "#5b8def",
        canvasBg: "#0f1626",
        textColor: "#b8c5d6",
        accentColor: "#5b8def",
        fontFamily: "'Segoe UI', system-ui, sans-serif",
        borderRadius: "8px",
        borderStyle: "solid",
    },
    obsidian: {
        name: "Obsidian",
        toolbar: "#1e1e1e",
        toolbarBorder: "#2d2d2d",
        buttonBg: "#2a2a2a",
        buttonActive: "#d4a054",
        canvasBg: "#171717",
        textColor: "#c4c4c4",
        accentColor: "#d4a054",
        fontFamily: "'Segoe UI', system-ui, sans-serif",
        borderRadius: "6px",
        borderStyle: "solid",
    },
    nord: {
        name: "Nord",
        toolbar: "#2e3440",
        toolbarBorder: "#3b4252",
        buttonBg: "#3b4252",
        buttonActive: "#88c0d0",
        canvasBg: "#242933",
        textColor: "#d8dee9",
        accentColor: "#88c0d0",
        fontFamily: "'Segoe UI', system-ui, sans-serif",
        borderRadius: "8px",
        borderStyle: "solid",
    },
    dracula: {
        name: "Dracula",
        toolbar: "#21222c",
        toolbarBorder: "#343746",
        buttonBg: "#2c2e3e",
        buttonActive: "#bd93f9",
        canvasBg: "#1a1b26",
        textColor: "#c5c8d4",
        accentColor: "#bd93f9",
        fontFamily: "'Segoe UI', system-ui, sans-serif",
        borderRadius: "8px",
        borderStyle: "solid",
    },
    monokai: {
        name: "Monokai",
        toolbar: "#272822",
        toolbarBorder: "#3e3d32",
        buttonBg: "#3e3d32",
        buttonActive: "#a6e22e",
        canvasBg: "#1e1f1c",
        textColor: "#c5c8c2",
        accentColor: "#a6e22e",
        fontFamily: "'Segoe UI', system-ui, sans-serif",
        borderRadius: "6px",
        borderStyle: "solid",
    },
};

const STORE_PATH = "settings.json";

export const loadThemeFromStore = async (): Promise<ThemeName> => {
    try {
        const store = await load(STORE_PATH);
        const saved = await store.get<string>("theme");
        if (saved && saved in THEMES) {
            return saved as ThemeName;
        }
    } catch {
        // Store not available
    }
    return "default";
};

export const saveThemeToStore = async (theme: ThemeName): Promise<void> => {
    try {
        const store = await load(STORE_PATH);
        await store.set("theme", theme);
        await store.save();
    } catch {
        // Store not available
    }
};
