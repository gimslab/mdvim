import { describe, it, expect } from 'vitest';
import {
  defaultConfig,
  parseTomlConfig,
  serializeTomlConfig,
  resolveShortcut,
  type MdvimConfig,
} from './index';

describe('mdvim configuration system', () => {
  it('resolves default shortcuts properly according to mode', () => {
    // Ctrl+Shift+P should always resolve to palette.open
    expect(resolveShortcut(defaultConfig, 'Ctrl+Shift+P', 'n')).toBe('palette.open');
    expect(resolveShortcut(defaultConfig, 'Ctrl+Shift+P', 'i')).toBe('palette.open');

    // Ctrl+B in insert mode should resolve to format.toggleBold
    expect(resolveShortcut(defaultConfig, 'Ctrl+B', 'i')).toBe('format.toggleBold');

    // Ctrl+B in normal mode has no override (falls through to Neovim)
    expect(resolveShortcut(defaultConfig, 'Ctrl+B', 'n')).toBe(null);

    // Case insensitivity & key order insensitivity (e.g. Shift+Ctrl+P)
    expect(resolveShortcut(defaultConfig, 'Shift+Ctrl+P', 'n')).toBe('palette.open');
  });

  it('parses custom shortcuts from TOML string', () => {
    const toml = `
[general]
locale = "en"
theme = "dark"

[shortcuts.always]
"Ctrl+Shift+P" = "palette.open"
"Ctrl+K" = "editor.toggleInspector"

[shortcuts.normal]
"Ctrl+D" = "nvim"

[shortcuts.insert]
"Ctrl+B" = "format.toggleBold"
"Ctrl+U" = "format.toggleCode"
`;

    const parsed = parseTomlConfig(toml);
    expect(parsed.locale).toBe('en');
    expect(parsed.theme).toBe('dark');
    expect(parsed.shortcuts?.length).toBe(5);

    const merged: MdvimConfig = {
      ...defaultConfig,
      ...parsed,
      shortcuts: parsed.shortcuts ?? defaultConfig.shortcuts,
    };

    expect(resolveShortcut(merged, 'Ctrl+K', 'n')).toBe('editor.toggleInspector');
    expect(resolveShortcut(merged, 'Ctrl+U', 'i')).toBe('format.toggleCode');
    expect(resolveShortcut(merged, 'Ctrl+U', 'n')).toBe(null);
  });

  it('serializes config roundtrip cleanly', () => {
    const toml = serializeTomlConfig(defaultConfig);
    expect(toml).toContain('[general]');
    expect(toml).toContain('locale = "ko"');
    expect(toml).toContain('[shortcuts.always]');
    expect(toml).toContain('"Ctrl+Shift+P" = "palette.open"');

    const parsed = parseTomlConfig(toml);
    expect(parsed.locale).toBe('ko');
    expect(parsed.shortcuts?.length).toBe(defaultConfig.shortcuts.length);
  });
});
