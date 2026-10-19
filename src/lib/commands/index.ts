export type CommandWhen = 'always' | 'normal' | 'insert' | 'visual';

export interface CommandContext {
  activeTab: {
    id: string;
    title: string;
    viewMode: 'rendered' | 'code';
    rawMarkdown: string;
    dirty: boolean;
  };
  cursor: {
    row: number;
    col: number;
  };
  mode: string;
  tabsCount: number;
  activeTabIndex: number;
}

export interface CommandDefinition<TArgs = any, TResult = any> {
  id: string;
  titleKo: string;
  titleEn: string;
  category: 'View' | 'Format' | 'Tab' | 'Editor' | 'Neovim';
  defaultShortcut?: string; // e.g. "Ctrl+Shift+P", "Ctrl+B"
  when?: CommandWhen;
  action: (ctx: CommandContext, args?: TArgs) => Promise<TResult> | TResult;
}

export class CommandRegistry {
  private commands = new Map<string, CommandDefinition>();

  register<TArgs, TResult>(cmd: CommandDefinition<TArgs, TResult>): void {
    this.commands.set(cmd.id, cmd);
  }

  unregisterPrefix(prefix: string): void {
    for (const key of Array.from(this.commands.keys())) {
      if (key.startsWith(prefix)) {
        this.commands.delete(key);
      }
    }
  }

  get(id: string): CommandDefinition | undefined {
    return this.commands.get(id);
  }

  list(): CommandDefinition[] {
    return Array.from(this.commands.values());
  }

  async execute<TArgs, TResult>(id: string, ctx: CommandContext, args?: TArgs): Promise<TResult> {
    const cmd = this.commands.get(id);
    if (!cmd) {
      throw new Error(`Command not found: ${id}`);
    }
    return await cmd.action(ctx, args);
  }

  search(query: string, lang: 'ko' | 'en' = 'ko'): CommandDefinition[] {
    const trimmed = query.trim().toLowerCase();
    if (!trimmed) {
      return this.list();
    }
    return this.list().filter((cmd) => {
      const title = lang === 'ko' ? cmd.titleKo : cmd.titleEn;
      const altTitle = lang === 'ko' ? cmd.titleEn : cmd.titleKo;
      return (
        cmd.id.toLowerCase().includes(trimmed) ||
        title.toLowerCase().includes(trimmed) ||
        altTitle.toLowerCase().includes(trimmed) ||
        cmd.category.toLowerCase().includes(trimmed)
      );
    });
  }
}
