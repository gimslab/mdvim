import type { Locale } from '../i18n';

export type ShortcutTarget = 'nvim' | 'app:toggle-bold' | 'app:toggle-italic' | 'app:toggle-View' | 'app:save';

export interface ShortcutRule {
  key: string; // e.g. "Ctrl+B", "Ctrl+I", "Ctrl+E", "Ctrl+S"
  normal: ShortcutTarget;
  insert: ShortcutTarget;
}

export interface MdvimConfig {
  locale: Locale;
  theme: 'light' | 'dark';
  autoSaveIntervalMs: number;
  autoSaveDebounceMs: number;
  autoSaveOnBlur: boolean;
  imagePasteDir: string; // e.g. "./assets"
  loadUserNvimConfig: boolean;
  shortcuts: ShortcutRule[];
}

export const defaultConfig: MdvimConfig = {
  locale: 'ko',
  theme: 'light',
  autoSaveIntervalMs: 30000,
  autoSaveDebounceMs: 1500,
  autoSaveOnBlur: true,
  imagePasteDir: './assets',
  loadUserNvimConfig: true,
  shortcuts: [
    // Per SPEC.md 5.3: Normal mode routes Ctrl keys to Neovim, Insert mode routes formatting shortcuts to App
    { key: 'Ctrl+B', normal: 'nvim', insert: 'app:toggle-bold' },
    { key: 'Ctrl+I', normal: 'nvim', insert: 'app:toggle-italic' },
    { key: 'Ctrl+E', normal: 'app:toggle-View', insert: 'app:toggle-View' },
    { key: 'Ctrl+S', normal: 'app:save', insert: 'app:save' },
  ],
};

export function resolveShortcut(
  config: MdvimConfig,
  combo: string,
  mode: string
): ShortcutTarget {
  const rule = config.shortcuts.find((r) => r.key.toLowerCase() === combo.toLowerCase());
  if (!rule) return 'nvim';
  const isInsert = mode.startsWith('i');
  return isInsert ? rule.insert : rule.normal;
}
