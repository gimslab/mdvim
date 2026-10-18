import { describe, it, expect } from 'vitest';
import { CommandRegistry, type CommandContext } from './index';

describe('CommandRegistry', () => {
  it('registers and executes commands', async () => {
    const reg = new CommandRegistry();
    let executed = false;

    reg.register({
      id: 'view.toggleMode',
      titleKo: '뷰 모드 전환 (렌더링 ↔ 코드)',
      titleEn: 'Toggle View Mode (Rendered ↔ Code)',
      category: 'View',
      defaultShortcut: 'Ctrl+E',
      action: () => {
        executed = true;
        return 'ok';
      },
    });

    const ctx: CommandContext = {
      activeTab: {
        id: 'tab-1',
        title: 'untitled.md',
        viewMode: 'rendered',
        rawMarkdown: '# Hi',
        dirty: false,
      },
      cursor: { row: 0, col: 0 },
      mode: 'n',
      tabsCount: 1,
      activeTabIndex: 0,
    };

    expect(reg.get('view.toggleMode')).toBeDefined();
    const result = await reg.execute('view.toggleMode', ctx);
    expect(executed).toBe(true);
    expect(result).toBe('ok');
  });

  it('searches commands in both Korean and English', () => {
    const reg = new CommandRegistry();

    reg.register({
      id: 'format.toggleBold',
      titleKo: '굵게 서식 토글',
      titleEn: 'Toggle Bold Format',
      category: 'Format',
      action: () => {},
    });

    reg.register({
      id: 'tab.new',
      titleKo: '새 탭 열기',
      titleEn: 'Open New Tab',
      category: 'Tab',
      action: () => {},
    });

    expect(reg.search('굵게', 'ko')).toHaveLength(1);
    expect(reg.search('bold', 'ko')).toHaveLength(1);
    expect(reg.search('tab', 'en')).toHaveLength(1);
    expect(reg.search('format')).toHaveLength(1);
    expect(reg.search('')).toHaveLength(2);
  });
});
