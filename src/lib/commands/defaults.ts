import { CommandRegistry, type CommandDefinition } from './index';

export function createDefaultRegistry(actions: {
  toggleViewMode: () => void | Promise<void>;
  switchViewMode: (mode: 'rendered' | 'code') => void | Promise<void>;
  toggleBold: () => void;
  toggleItalic: () => void;
  toggleStrikethrough?: () => void;
  toggleCode: () => void;
  toggleCheckbox?: () => void;
  newTab: () => void | Promise<void>;
  nextTab: () => void | Promise<void>;
  prevTab: () => void | Promise<void>;
  closeActiveTab: () => void | Promise<void>;
  toggleInspector: () => void;
  openFile?: () => void | Promise<void>;
  saveFile?: () => void | Promise<void>;
  saveFileAs?: () => void | Promise<void>;
}): CommandRegistry {
  const registry = new CommandRegistry();

  const commands: CommandDefinition[] = [
    {
      id: 'view.toggleMode',
      titleKo: '뷰 전환: 렌더링 뷰 ⟷ 코드 뷰 토글',
      titleEn: 'View: Toggle Rendered / Code View',
      category: 'View',
      defaultShortcut: 'Ctrl+E',
      action: () => actions.toggleViewMode(),
    },
    {
      id: 'view.setRendered',
      titleKo: '뷰: 렌더링 뷰로 전환',
      titleEn: 'View: Switch to Rendered View',
      category: 'View',
      action: () => actions.switchViewMode('rendered'),
    },
    {
      id: 'view.setCode',
      titleKo: '뷰: 마크다운 코드 뷰로 전환',
      titleEn: 'View: Switch to Code View',
      category: 'View',
      action: () => actions.switchViewMode('code'),
    },
    {
      id: 'editor.toggleInspector',
      titleKo: '에디터: 마크다운 실시간 원본 패널 토글',
      titleEn: 'Editor: Toggle Markdown Inspector Panel',
      category: 'Editor',
      action: () => actions.toggleInspector(),
    },
    {
      id: 'format.toggleBold',
      titleKo: '서식: 굵게 (Bold) 토글',
      titleEn: 'Format: Toggle Bold',
      category: 'Format',
      defaultShortcut: 'Ctrl+B',
      action: () => actions.toggleBold(),
    },
    {
      id: 'format.toggleItalic',
      titleKo: '서식: 기울임 (Italic) 토글',
      titleEn: 'Format: Toggle Italic',
      category: 'Format',
      defaultShortcut: 'Ctrl+I',
      action: () => actions.toggleItalic(),
    },
    {
      id: 'format.toggleStrikethrough',
      titleKo: '서식: 취소선 (Strikethrough) 토글',
      titleEn: 'Format: Toggle Strikethrough',
      category: 'Format',
      defaultShortcut: 'Ctrl+D',
      action: () => actions.toggleStrikethrough?.(),
    },
    {
      id: 'format.toggleCode',
      titleKo: '서식: 인라인 코드 (Code) 토글',
      titleEn: 'Format: Toggle Inline Code',
      category: 'Format',
      action: () => actions.toggleCode(),
    },
    {
      id: 'format.toggleCheckbox',
      titleKo: '서식: 할 일 체크박스 완료 토글',
      titleEn: 'Format: Toggle Task Checkbox',
      category: 'Format',
      defaultShortcut: 'Ctrl+L',
      action: () => actions.toggleCheckbox?.(),
    },
    {
      id: 'tab.new',
      titleKo: '탭: 새 문서 탭 열기',
      titleEn: 'Tab: New Tab',
      category: 'Tab',
      defaultShortcut: 'Ctrl+T',
      action: () => actions.newTab(),
    },
    {
      id: 'tab.next',
      titleKo: '탭: 다음 탭으로 이동',
      titleEn: 'Tab: Next Tab',
      category: 'Tab',
      defaultShortcut: 'Ctrl+Tab',
      action: () => actions.nextTab(),
    },
    {
      id: 'tab.prev',
      titleKo: '탭: 이전 탭으로 이동',
      titleEn: 'Tab: Previous Tab',
      category: 'Tab',
      defaultShortcut: 'Ctrl+Shift+Tab',
      action: () => actions.prevTab(),
    },
    {
      id: 'tab.close',
      titleKo: '탭: 현재 탭 닫기',
      titleEn: 'Tab: Close Current Tab',
      category: 'Tab',
      defaultShortcut: 'Ctrl+W',
      action: () => actions.closeActiveTab(),
    },
  ];

  if (actions.openFile) {
    commands.push({
      id: 'file.open',
      titleKo: '파일: 문서 열기',
      titleEn: 'File: Open Document',
      category: 'Editor',
      defaultShortcut: 'Ctrl+O',
      action: () => actions.openFile!(),
    });
  }

  if (actions.saveFile) {
    commands.push({
      id: 'file.save',
      titleKo: '파일: 문서 저장',
      titleEn: 'File: Save Document',
      category: 'Editor',
      defaultShortcut: 'Ctrl+S',
      action: () => actions.saveFile!(),
    });
  }

  if (actions.saveFileAs) {
    commands.push({
      id: 'file.saveAs',
      titleKo: '파일: 다른 이름으로 저장',
      titleEn: 'File: Save Document As...',
      category: 'Editor',
      defaultShortcut: 'Ctrl+Shift+S',
      action: () => actions.saveFileAs!(),
    });
  }

  for (const cmd of commands) {
    registry.register(cmd);
  }

  return registry;
}
