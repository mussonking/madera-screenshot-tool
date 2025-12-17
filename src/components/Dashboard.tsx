import { useState } from 'react';
import { Camera, Palette, History as HistoryIcon, Shield } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import History from './History';

export default function Dashboard() {
  const [showHistory, setShowHistory] = useState(false);

  const tools = [
    {
      id: 'screenshot',
      name: 'Screenshot Tool',
      icon: Camera,
      description: 'Capture and edit screenshots',
      shortcut: 'Ctrl+Shift+S',
      action: () => invoke('trigger_screenshot_capture'),
      gradient: 'from-blue-500 to-cyan-500',
    },
    {
      id: 'color-picker',
      name: 'Color Picker',
      icon: Palette,
      description: 'Pick colors from your screen',
      shortcut: 'Ctrl+Shift+X',
      action: () => invoke('open_color_picker_panel'),
      gradient: 'from-purple-500 to-pink-500',
    },
    {
      id: 'history',
      name: 'History Manager',
      icon: HistoryIcon,
      description: 'View clipboard & screenshot history',
      shortcut: 'Ctrl+Shift+V',
      action: () => setShowHistory(!showHistory),
      gradient: 'from-green-500 to-emerald-500',
    },
    {
      id: 'desktop-guardian',
      name: 'Desktop Guardian',
      icon: Shield,
      description: 'Save & restore window layouts',
      shortcut: '',
      action: () => invoke('open_desktop_guardian_panel'),
      gradient: 'from-orange-500 to-red-500',
    },
  ];

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 text-white p-8">
      {/* Header */}
      <div className="max-w-6xl mx-auto mb-12">
        <h1 className="text-5xl font-bold mb-4 bg-gradient-to-r from-blue-400 to-purple-400 bg-clip-text text-transparent">
          Madera.Tools
        </h1>
        <p className="text-slate-400 text-lg">
          Professional productivity suite for developers and power users
        </p>
      </div>

      {/* Tools Grid */}
      <div className="max-w-6xl mx-auto grid grid-cols-1 md:grid-cols-2 gap-6 mb-12">
        {tools.map((tool) => {
          const Icon = tool.icon;
          return (
            <button
              key={tool.id}
              onClick={tool.action}
              className="group relative bg-slate-800/50 backdrop-blur-sm rounded-2xl p-8 border border-slate-700 hover:border-slate-600 transition-all duration-300 hover:scale-105 hover:shadow-2xl text-left overflow-hidden"
            >
              {/* Gradient background on hover */}
              <div className={`absolute inset-0 bg-gradient-to-br ${tool.gradient} opacity-0 group-hover:opacity-10 transition-opacity duration-300`} />

              <div className="relative z-10">
                <div className="flex items-start justify-between mb-4">
                  <div className={`p-4 rounded-xl bg-gradient-to-br ${tool.gradient} shadow-lg`}>
                    <Icon className="w-8 h-8" />
                  </div>
                  {tool.shortcut && (
                    <div className="px-3 py-1 bg-slate-700/50 rounded-lg text-xs font-mono text-slate-300">
                      {tool.shortcut}
                    </div>
                  )}
                </div>

                <h2 className="text-2xl font-bold mb-2">{tool.name}</h2>
                <p className="text-slate-400">{tool.description}</p>
              </div>
            </button>
          );
        })}
      </div>

      {/* History Section */}
      {showHistory && (
        <div className="max-w-6xl mx-auto">
          <div className="bg-slate-800/50 backdrop-blur-sm rounded-2xl p-6 border border-slate-700">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-2xl font-bold">Recent History</h2>
              <button
                onClick={() => setShowHistory(false)}
                className="text-slate-400 hover:text-white transition-colors"
              >
                Hide
              </button>
            </div>
            <History />
          </div>
        </div>
      )}

      {/* Footer */}
      <div className="max-w-6xl mx-auto mt-12 text-center text-slate-500 text-sm">
        <p>Madera.Tools v1.0.0 • Professional Edition</p>
      </div>
    </div>
  );
}
