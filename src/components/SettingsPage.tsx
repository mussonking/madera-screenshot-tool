import { invoke } from '@tauri-apps/api/core';
import SettingsModal from './SettingsModal';

export default function SettingsPage() {
    const handleClose = () => {
        // Close the window since it's a standalone view
        invoke('close_window', { label: 'settings' }).catch((err) => {
            console.error('Failed to close settings window', err);
            window.close();
        });
    };

    return (
        <div className="h-full w-full bg-slate-950">
            <SettingsModal isOpen={true} onClose={handleClose} />
        </div>
    );
}
