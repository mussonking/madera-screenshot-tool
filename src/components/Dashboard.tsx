import { useState, useEffect } from 'react';
import { Camera, Palette, Shield, LayoutDashboard, Settings, Copy, Clipboard } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import History from './History';
import SettingsModal from './SettingsModal';
import { THEMES, ThemeName, loadThemeFromStore } from '../utils/theme';

export default function Dashboard() {
  const [showSettings, setShowSettings] = useState(false);
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");

  useEffect(() => {
    loadThemeFromStore().then(setCurrentTheme);
  }, []);

  const theme = THEMES[currentTheme];

  const tools = [
    {
      id: 'screenshot',
      name: 'Capture',
      icon: Camera,
      action: () => invoke('trigger_capture'),
    },
    {
      id: 'color-picker',
      name: 'Colors',
      icon: Palette,
      action: () => invoke('trigger_color_picker'),
    },
    {
      id: 'desktop-guardian',
      name: 'Guardian',
      icon: Shield,
      action: () => invoke('open_desktop_guardian'),
    },
    {
      id: 'quick-paste',
      name: 'Quick Paste',
      icon: Clipboard,
      action: () => invoke('open_multi_paste_panel'),
    },
    {
      id: 'prompt-snippets',
      name: 'Prompt Snippets',
      icon: Copy,
      action: () => invoke('open_quick_paste_panel'),
    },
  ];

  return (
    <div
      className="h-screen w-full flex flex-col overflow-hidden"
      style={{
        backgroundColor: theme.canvasBg,
        color: theme.textColor,
        fontFamily: theme.fontFamily,
      }}
    >
      {/* Top Toolbar */}
      <div
        className="shrink-0 flex items-center justify-between p-4 border-b"
        style={{
          backgroundColor: theme.toolbar,
          borderColor: theme.toolbarBorder,
        }}
      >
        <div className="flex items-center gap-3">
          <LayoutDashboard size={24} style={{ color: theme.accentColor }} />
          <h1 className="text-xl font-bold">
            Madera.Tools
          </h1>
        </div>

        <div className="flex items-center gap-2">
          {/* Quick Tools */}
          {tools.map((tool) => {
            const Icon = tool.icon;
            return (
              <button
                key={tool.id}
                onClick={tool.action}
                style={{
                  backgroundColor: theme.buttonBg,
                  borderRadius: theme.borderRadius,
                }}
                className="flex items-center gap-2 px-3 py-2 hover:opacity-80 transition-colors text-sm font-medium"
                title={tool.name}
              >
                <Icon size={16} />
                <span className="hidden sm:inline">{tool.name}</span>
              </button>
            );
          })}

          <div className="w-px h-6 mx-2" style={{ backgroundColor: theme.toolbarBorder }} />

          {/* Settings Button */}
          <button
            onClick={() => setShowSettings(true)}
            style={{
              backgroundColor: theme.buttonBg,
              borderRadius: theme.borderRadius,
            }}
            className="p-2 hover:opacity-80 transition-colors"
            title="Settings"
          >
            <Settings size={18} />
          </button>
        </div>
      </div>

      {/* Main Content Area - Full screen History */}
      <History />

      {/* Settings Modal */}
      <SettingsModal
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
      />
    </div>
  );
}
