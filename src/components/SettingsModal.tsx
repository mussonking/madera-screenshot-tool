import { useState, useEffect } from 'react';
import { Settings, Copy, Monitor, Palette, Upload, Keyboard } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { THEMES, ThemeName, loadThemeFromStore, saveThemeToStore } from '../utils/theme';

interface SettingsModalProps {
    isOpen: boolean;
    onClose: () => void;
    // Passing these so the modal can update parent state if needed, or it can fetch its own
}

interface AppSettings {
    hotkey: string;
    auto_copy: boolean;
    max_history: number;
    max_image_width: number | null;
    ssh_enabled: boolean;
    ssh_host: string;
    ssh_remote_path: string;
}

interface ClipboardSettings {
    enabled: boolean;
    max_items: number;
    excluded_apps: string[];
    auto_cleanup_days: number | null;
}



export default function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
    const [activeTab, setActiveTab] = useState<'general' | 'appearance' | 'clipboard' | 'ssh'>('general');
    const [isAutostart, setIsAutostart] = useState(false);

    const [appSettings, setAppSettings] = useState<AppSettings>({
        hotkey: 'Ctrl+Shift+S',
        auto_copy: true,
        max_history: 150,
        max_image_width: 1568,
        ssh_enabled: false,
        ssh_host: '',
        ssh_remote_path: '',
    });

    const [clipboardSettings, setClipboardSettings] = useState<ClipboardSettings>({
        enabled: true,
        max_items: 200,
        excluded_apps: [],
        auto_cleanup_days: 30,
    });

    const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");

    useEffect(() => {
        if (isOpen) {
            loadAllSettings();
        }
    }, [isOpen]);

    const loadAllSettings = async () => {
        try {
            // Load Settings
            const loadedApp = await invoke<AppSettings>('get_settings');
            setAppSettings(loadedApp);

            const loadedClip = await invoke<ClipboardSettings>('get_clipboard_settings');
            setClipboardSettings(loadedClip);

            const autostart = await invoke<boolean>('is_autostart_enabled');
            setIsAutostart(autostart);

            // Load Theme
            const initialTheme = await loadThemeFromStore();
            setCurrentTheme(initialTheme);

        } catch (err) {
            console.error('Failed to load settings:', err);
        }
    };

    const saveAppSettings = async (newSettings: AppSettings) => {
        setAppSettings(newSettings);
        try {
            await invoke('update_settings', { settings: newSettings });
        } catch (err) {
            console.error('Failed to save app settings:', err);
        }
    };

    const saveClipboardSettings = async (newSettings: ClipboardSettings) => {
        setClipboardSettings(newSettings);
        try {
            await invoke('update_clipboard_settings', { settings: newSettings });
        } catch (err) {
            console.error('Failed to save clipboard settings:', err);
        }
    };

    const handleToggleAutostart = async () => {
        try {
            await invoke('toggle_autostart', { enabled: !isAutostart });
            setIsAutostart(!isAutostart);
        } catch (err) {
            console.error('Failed to toggle autostart:', err);
        }
    };

    const handleThemeChange = async (theme: ThemeName) => {
        setCurrentTheme(theme);
        try {
            await saveThemeToStore(theme);
            // Reload to apply theme everywhere
            window.location.reload();
        } catch (e) {
            console.error(e);
        }
    }

    if (!isOpen) return null;

    const theme = THEMES[currentTheme];

    return (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
            <div
                className="bg-slate-900 border rounded-2xl w-full max-w-3xl h-[80vh] flex flex-col overflow-hidden shadow-2xl"
                style={{ backgroundColor: theme.canvasBg, borderColor: theme.toolbarBorder, color: theme.textColor, fontFamily: theme.fontFamily }}
            >
                {/* Header */}
                <div
                    className="flex items-center justify-between p-6 border-b"
                    style={{ backgroundColor: theme.toolbar, borderColor: theme.toolbarBorder }}
                >
                    <div className="flex items-center gap-3">
                        <Settings style={{ color: theme.accentColor }} size={24} />
                        <h2 className="text-2xl font-bold" style={{ color: theme.textColor }}>Application Settings</h2>
                    </div>
                    <button
                        onClick={onClose}
                        className="text-slate-400 hover:text-white transition-colors p-2 hover:bg-slate-700 rounded-lg"
                    >
                        ✕
                    </button>
                </div>

                <div className="flex-1 flex overflow-hidden">
                    {/* Sidebar Tabs */}
                    <div
                        className="w-48 border-r p-4 flex flex-col gap-2"
                        style={{ backgroundColor: theme.buttonBg, borderColor: theme.toolbarBorder }}
                    >
                        <TabButton
                            active={activeTab === 'general'}
                            onClick={() => setActiveTab('general')}
                            icon={<Monitor size={18} />}
                            label="General"
                        />
                        <TabButton
                            active={activeTab === 'appearance'}
                            onClick={() => setActiveTab('appearance')}
                            icon={<Palette size={18} />}
                            label="Appearance"
                        />
                        <TabButton
                            active={activeTab === 'clipboard'}
                            onClick={() => setActiveTab('clipboard')}
                            icon={<Copy size={18} />}
                            label="Clipboard"
                        />
                        <TabButton
                            active={activeTab === 'ssh'}
                            onClick={() => setActiveTab('ssh')}
                            icon={<Upload size={18} />}
                            label="SSH Upload"
                        />
                    </div>

                    {/* Content Area */}
                    <div className="flex-1 overflow-y-auto p-8" style={{ backgroundColor: theme.canvasBg }}>

                        {activeTab === 'general' && (
                            <div className="space-y-8 animate-in fade-in slide-in-from-right-4 duration-300">
                                <section>
                                    <h3 className="text-lg font-medium text-white mb-4 flex items-center gap-2">
                                        <Monitor className="text-blue-400" size={20} /> System
                                    </h3>
                                    <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700/50">
                                        <label className="flex items-center justify-between cursor-pointer">
                                            <div>
                                                <div className="text-white font-medium">Start with Windows</div>
                                                <div className="text-sm text-slate-400">Launch Madera.Tools automatically when you log in</div>
                                            </div>
                                            <div className={`w-11 h-6 rounded-full transition-colors relative ${isAutostart ? 'bg-blue-500' : 'bg-slate-600'}`}>
                                                <div className={`absolute top-1 w-4 h-4 rounded-full bg-white transition-transform ${isAutostart ? 'left-6' : 'left-1'}`} />
                                                <input type="checkbox" className="hidden" checked={isAutostart} onChange={handleToggleAutostart} />
                                            </div>
                                        </label>
                                    </div>
                                </section>

                                <section>
                                    <h3 className="text-lg font-medium text-white mb-4 flex items-center gap-2">
                                        <Keyboard className="text-emerald-400" size={20} /> Keyboard Shortcuts
                                    </h3>
                                    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
                                        <ShortcutRow label="Take Screenshot" shortcut="Ctrl+Shift+S" />
                                        <ShortcutRow label="Open Color Picker" shortcut="Ctrl+Shift+X" />
                                        <ShortcutRow label="Open History Hub" shortcut="Ctrl+Shift+H" />
                                        <ShortcutRow label="Open Quick Paste" shortcut="Ctrl+Alt+V" />
                                        <ShortcutRow label="Double Paste (Last Item)" shortcut="Ctrl+V (Double Tap)" />
                                    </div>
                                    <p className="text-sm text-slate-500 mt-2 ml-1">Shortcuts are currently read-only.</p>
                                </section>
                            </div>
                        )}

                        {activeTab === 'appearance' && (
                            <div className="space-y-8 animate-in fade-in slide-in-from-right-4 duration-300">
                                <section>
                                    <h3 className="text-lg font-medium text-white mb-4 flex items-center gap-2">
                                        <Palette className="text-purple-400" size={20} /> Application Theme
                                    </h3>
                                    <div className="grid grid-cols-2 gap-4">
                                        {(Object.keys(THEMES) as ThemeName[]).map((themeName) => (
                                            <button
                                                key={themeName}
                                                onClick={() => handleThemeChange(themeName)}
                                                className={`p-4 rounded-xl border flex flex-col items-center gap-3 transition-all ${currentTheme === themeName
                                                    ? 'border-blue-500 bg-blue-500/10'
                                                    : 'border-slate-700 bg-slate-800/50 hover:border-slate-500 hover:bg-slate-700/50'
                                                    }`}
                                            >
                                                <div className="font-medium text-white">{THEMES[themeName].name}</div>
                                            </button>
                                        ))}
                                    </div>
                                    <p className="text-sm text-amber-500 mt-4 bg-amber-500/10 p-3 rounded-lg border border-amber-500/20">
                                        Changing the theme will reload the application interface to apply global styles.
                                    </p>
                                </section>
                            </div>
                        )}

                        {activeTab === 'clipboard' && (
                            <div className="space-y-8 animate-in fade-in slide-in-from-right-4 duration-300">
                                <section>
                                    <h3 className="text-lg font-medium text-white mb-4 flex items-center gap-2">
                                        <Copy className="text-amber-400" size={20} /> Clipboard History
                                    </h3>

                                    <div className="space-y-4">
                                        <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700/50">
                                            <label className="flex items-center justify-between cursor-pointer">
                                                <div>
                                                    <div className="text-white font-medium">Enable Clipboard Monitoring</div>
                                                    <div className="text-sm text-slate-400">Automatically save copied text and images to history</div>
                                                </div>
                                                <div className={`w-11 h-6 rounded-full transition-colors relative ${clipboardSettings.enabled ? 'bg-amber-500' : 'bg-slate-600'}`}>
                                                    <div className={`absolute top-1 w-4 h-4 rounded-full bg-white transition-transform ${clipboardSettings.enabled ? 'left-6' : 'left-1'}`} />
                                                    <input
                                                        type="checkbox"
                                                        className="hidden"
                                                        checked={clipboardSettings.enabled}
                                                        onChange={(e) => saveClipboardSettings({ ...clipboardSettings, enabled: e.target.checked })}
                                                    />
                                                </div>
                                            </label>
                                        </div>

                                        <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700/50 space-y-4">
                                            <div>
                                                <div className="flex justify-between mb-2">
                                                    <label className="text-white font-medium">Maximum History Items</label>
                                                    <span className="text-amber-400 font-mono">{clipboardSettings.max_items}</span>
                                                </div>
                                                <input
                                                    type="range"
                                                    min="50" max="1000" step="50"
                                                    value={clipboardSettings.max_items}
                                                    onChange={(e) => saveClipboardSettings({ ...clipboardSettings, max_items: parseInt(e.target.value) })}
                                                    className="w-full accent-amber-500"
                                                />
                                                <div className="flex justify-between text-xs text-slate-500 mt-1">
                                                    <span>50</span>
                                                    <span>1000</span>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </section>
                            </div>
                        )}

                        {activeTab === 'ssh' && (
                            <div className="space-y-8 animate-in fade-in slide-in-from-right-4 duration-300">
                                <section>
                                    <h3 className="text-lg font-medium text-white mb-4 flex items-center gap-2">
                                        <Upload className="text-cyan-400" size={20} /> Remote Server Upload
                                    </h3>

                                    <div className="space-y-4">
                                        <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700/50">
                                            <label className="flex items-center justify-between cursor-pointer">
                                                <div>
                                                    <div className="text-white font-medium">Enable SSH Upload</div>
                                                    <div className="text-sm text-slate-400">Show upload button in editor to send images to remote server</div>
                                                </div>
                                                <div className={`w-11 h-6 rounded-full transition-colors relative ${appSettings.ssh_enabled ? 'bg-cyan-500' : 'bg-slate-600'}`}>
                                                    <div className={`absolute top-1 w-4 h-4 rounded-full bg-white transition-transform ${appSettings.ssh_enabled ? 'left-6' : 'left-1'}`} />
                                                    <input
                                                        type="checkbox"
                                                        className="hidden"
                                                        checked={appSettings.ssh_enabled}
                                                        onChange={(e) => saveAppSettings({ ...appSettings, ssh_enabled: e.target.checked })}
                                                    />
                                                </div>
                                            </label>
                                        </div>

                                        <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700/50 space-y-4">
                                            <div>
                                                <label className="block text-sm font-medium text-slate-300 mb-2">
                                                    SSH Host (IP or Hostname)
                                                </label>
                                                <input
                                                    type="text"
                                                    value={appSettings.ssh_host}
                                                    onChange={(e) => saveAppSettings({ ...appSettings, ssh_host: e.target.value })}
                                                    placeholder="e.g. user@192.168.1.100"
                                                    disabled={!appSettings.ssh_enabled}
                                                    className="w-full px-4 py-2 bg-slate-900/80 border border-slate-600 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500 disabled:opacity-50"
                                                />
                                            </div>

                                            <div>
                                                <label className="block text-sm font-medium text-slate-300 mb-2">
                                                    Remote Directory Path
                                                </label>
                                                <input
                                                    type="text"
                                                    value={appSettings.ssh_remote_path}
                                                    onChange={(e) => saveAppSettings({ ...appSettings, ssh_remote_path: e.target.value })}
                                                    placeholder="/home/user/downloads"
                                                    disabled={!appSettings.ssh_enabled}
                                                    className="w-full px-4 py-2 bg-slate-900/80 border border-slate-600 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500 disabled:opacity-50"
                                                />
                                            </div>

                                        </div>
                                    </div>
                                </section>
                            </div>
                        )}

                    </div>
                </div>
            </div>
        </div>
    );
}

function TabButton({ active, onClick, icon, label }: { active: boolean, onClick: () => void, icon: React.ReactNode, label: string }) {
    return (
        <button
            onClick={onClick}
            className={`flex items-center gap-3 px-4 py-3 rounded-xl transition-all text-sm font-medium w-full text-left
        ${active
                    ? 'bg-blue-500/10 text-blue-400 border border-blue-500/20 shadow-sm'
                    : 'text-slate-400 hover:bg-slate-800 hover:text-slate-200 border border-transparent'
                }
      `}
        >
            {icon}
            {label}
        </button>
    );
}

function ShortcutRow({ label, shortcut }: { label: string, shortcut: string }) {
    return (
        <div className="flex items-center justify-between p-3 border-b border-slate-700/50 last:border-0 hover:bg-slate-800/30">
            <span className="text-slate-300">{label}</span>
            <kbd className="px-2.5 py-1 bg-slate-900 border border-slate-700 rounded-md text-xs font-mono text-slate-300 shadow-sm">
                {shortcut}
            </kbd>
        </div>
    );
}
