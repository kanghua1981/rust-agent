import React from 'react';

export type SettingsSection = 'general' | 'models' | 'plugins';

const SECTIONS: { id: SettingsSection; icon: string; label: string }[] = [
  { id: 'general', icon: '⚙️', label: '常规' },
  { id: 'models',  icon: '🧠', label: '模型' },
  { id: 'plugins', icon: '🧩', label: '插件' },
];

interface Props {
  section: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
  children: React.ReactNode;
}

/**
 * Settings area container: the section strip plus the selected section's body.
 * The host supplies the body so it keeps ownership of the WebSocket handlers
 * each management panel needs.
 */
export const SettingsShell: React.FC<Props> = ({ section, onSectionChange, children }) => (
  <div className="settings-shell">
    <div className="subtabs root">
      {SECTIONS.map(s => (
        <button
          key={s.id}
          className={`subtab${section === s.id ? ' active' : ''}`}
          onClick={() => onSectionChange(s.id)}
        >
          {s.icon} {s.label}
        </button>
      ))}
    </div>
    <div className="settings-content">{children}</div>
  </div>
);
