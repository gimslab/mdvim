import type { Locale } from '../i18n';

export type ShortcutTarget = string; // Command ID, e.g. "view.toggleMode", "format.toggleBold", "palette.open", or "nvim"

export interface ShortcutRule {
  key: string; // e.g. "Ctrl+B", "Ctrl+Shift+P", "F1"
  when?: 'always' | 'normal' | 'insert' | 'visual';
  command: ShortcutTarget;
}

export interface MdvimConfig {
  locale: Locale;
  theme: 'light' | 'dark';
  autoSaveIntervalMs: number;
  autoSaveDebounceMs: number;
  autoSaveOnBlur: boolean;
  imagePasteDir: string;
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
    { key: 'Ctrl+Shift+P', when: 'always', command: 'palette.open' },
    { key: 'F1', when: 'always', command: 'palette.open' },
    { key: 'Ctrl+E', when: 'always', command: 'view.toggleMode' },
    { key: 'Ctrl+B', when: 'insert', command: 'format.toggleBold' },
    { key: 'Ctrl+I', when: 'insert', command: 'format.toggleItalic' },
    { key: 'Ctrl+T', when: 'always', command: 'tab.new' },
    { key: 'Ctrl+W', when: 'always', command: 'tab.close' },
    { key: 'Ctrl+Tab', when: 'always', command: 'tab.next' },
    { key: 'Ctrl+Shift+Tab', when: 'always', command: 'tab.prev' },
    { key: 'Ctrl+O', when: 'always', command: 'file.open' },
    { key: 'Ctrl+S', when: 'always', command: 'file.save' },
    { key: 'Ctrl+Shift+S', when: 'always', command: 'file.saveAs' },
  ],
};

function normalizeKey(key: string): string {
  return key
    .split('+')
    .map((p) => p.trim())
    .map((p) => p.toLowerCase())
    .sort()
    .join('+');
}

export function parseTomlConfig(tomlStr: string): Partial<MdvimConfig> {
  const result: Partial<MdvimConfig> = {
    shortcuts: [],
  };

  const lines = tomlStr.split('\n');
  let currentSection = '';

  for (let rawLine of lines) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) continue;

    const sectionMatch = /^\[(.*?)\]$/.exec(line);
    if (sectionMatch) {
      currentSection = sectionMatch[1].trim();
      continue;
    }

    const kvMatch = /^(.*?)=(.*)$/.exec(line);
    if (!kvMatch) continue;

    let key = kvMatch[1].trim();
    if (key.startsWith('"') && key.endsWith('"')) {
      key = key.slice(1, -1);
    } else if (key.startsWith("'") && key.endsWith("'")) {
      key = key.slice(1, -1);
    }
    let val = kvMatch[2].trim();
    if (val.startsWith('"') && val.endsWith('"')) {
      val = val.slice(1, -1);
    } else if (val.startsWith("'") && val.endsWith("'")) {
      val = val.slice(1, -1);
    }

    if (!currentSection || currentSection === 'general') {
      if (key === 'locale' && (val === 'ko' || val === 'en')) {
        result.locale = val;
      } else if (key === 'theme' && (val === 'light' || val === 'dark')) {
        result.theme = val;
      } else if (key === 'autoSaveIntervalMs') {
        result.autoSaveIntervalMs = Number(val);
      } else if (key === 'autoSaveDebounceMs') {
        result.autoSaveDebounceMs = Number(val);
      } else if (key === 'autoSaveOnBlur') {
        result.autoSaveOnBlur = val === 'true';
      }
    } else if (currentSection.startsWith('shortcuts')) {
      // E.g. [shortcuts.always] or [shortcuts.insert] or [shortcuts.normal]
      const sub = currentSection.split('.')[1] || 'always';
      const when = (['always', 'normal', 'insert', 'visual'].includes(sub)
        ? sub
        : 'always') as 'always' | 'normal' | 'insert' | 'visual';
      result.shortcuts!.push({
        key,
        when,
        command: val,
      });
    }
  }

  return result;
}

export function serializeTomlConfig(cfg: MdvimConfig): string {
  const lines: string[] = [
    '# mdvim User Configuration (~/.config/mdvim/config.toml)',
    '',
    '[general]',
    `locale = "${cfg.locale}"`,
    `theme = "${cfg.theme}"`,
    `autoSaveIntervalMs = ${cfg.autoSaveIntervalMs}`,
    `autoSaveDebounceMs = ${cfg.autoSaveDebounceMs}`,
    `autoSaveOnBlur = ${cfg.autoSaveOnBlur}`,
    '',
  ];

  const sections: Record<string, ShortcutRule[]> = {
    always: [],
    normal: [],
    insert: [],
    visual: [],
  };

  for (const s of cfg.shortcuts) {
    const sec = s.when || 'always';
    if (!sections[sec]) sections[sec] = [];
    sections[sec].push(s);
  }

  for (const [secName, rules] of Object.entries(sections)) {
    if (rules.length === 0) continue;
    lines.push(`[shortcuts.${secName}]`);
    for (const r of rules) {
      lines.push(`"${r.key}" = "${r.command}"`);
    }
    lines.push('');
  }

  return lines.join('\n');
}

export function resolveShortcut(
  config: MdvimConfig,
  combo: string,
  mode: string
): string | null {
  const normCombo = normalizeKey(combo);
  const isInsert = mode.startsWith('i');
  const isVisual = mode.startsWith('v') || mode.startsWith('V');

  // 1. Check mode-specific rule first
  const specificRule = config.shortcuts.find((r) => {
    if (normalizeKey(r.key) !== normCombo) return false;
    if (isInsert && r.when === 'insert') return true;
    if (isVisual && r.when === 'visual') return true;
    if (!isInsert && !isVisual && r.when === 'normal') return true;
    return false;
  });

  if (specificRule) {
    return specificRule.command;
  }

  // 2. Check 'always' (global) rule
  const globalRule = config.shortcuts.find(
    (r) => (r.when === 'always' || !r.when) && normalizeKey(r.key) === normCombo
  );

  return globalRule ? globalRule.command : null;
}
