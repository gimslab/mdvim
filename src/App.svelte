<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
  import { defaultConfig, resolveShortcut, parseTomlConfig, type MdvimConfig } from './lib/config';
  import { t } from './lib/i18n';
  import {
    parseMarkdown,
    projectLines,
    serializeMarkdown,
    extractMarkdownRange,
    applyProjected,
    reparseCompletedLines,
    isHorizontalRulePrefix,
    type ProjectedLine,
    type StyledChar,
  } from './lib/projection';
  import { createDefaultRegistry } from './lib/commands/defaults';
  import type { CommandDefinition, CommandContext } from './lib/commands';
  import CommandPalette from './components/CommandPalette.svelte';

  interface YankEvent {
    regname: string;
    regtype: string;
    regcontents: string[];
    start_row: number;
    start_col: number;
    end_row: number;
    end_col: number;
    operator: string;
  }

  interface NvimState {
    lines: string[];
    cursor_row: number;
    cursor_col_byte: number;
    cursor_col_char: number;
    mode: string;
    blocking: boolean;
    cmdline?: string;
    cmdpos?: number;
    last_yank?: YankEvent | null;
    ex_command?: string | null;
  }

  interface TabDoc {
    id: string;
    title: string;
    path: string | null;
    viewMode: 'rendered' | 'code';
    mdModel: ProjectedLine[];
    dirty: boolean;
  }

  interface RenderSegment {
    text: string;
    cls: string;
    isPreedit?: boolean;
  }

  let config = $state({ ...defaultConfig });
  let msg = $derived(t(config.locale));
  let showInspector = $state(true);
  let showCommandPalette = $state(false);

  let tabs = $state<TabDoc[]>([
    {
      id: 'tab-1',
      title: 'untitled.md',
      path: null,
      viewMode: 'rendered',
      mdModel: parseMarkdown(
        [
          '# mdvim 에디터',
          '',
          '렌더링 뷰에서 **굵은 글씨**, *기울임*, `인라인 코드`와 Vim 모션을 바로 테스트해 보세요.',
          '- 첫 번째 목록 **중요** 항목',
          '- [ ] 할 일 체크박스 항목 (클릭하여 완료 토글)',
          '',
          '빈 줄에서 `# 제목` 또는 `**강조**`를 입력하고 Enter나 Esc를 눌러 보세요!',
        ].join('\n')
      ),
      dirty: false,
    },
  ]);
  let activeTabIndex = $state(0);
  let activeTab = $derived(tabs[activeTabIndex]);
  let rawMarkdownPreview = $derived(serializeMarkdown(activeTab.mdModel));

  let nvimState = $state<NvimState>({
    lines: [],
    cursor_row: 0,
    cursor_col_byte: 0,
    cursor_col_char: 0,
    mode: 'n',
    blocking: false,
  });

  let isComposing = $state(false);
  let preeditText = $state('');
  let imeInputEl: HTMLTextAreaElement | undefined = $state();

  let rpcQueue = Promise.resolve();
  function enqueueRpc(fn: () => Promise<void>) {
    rpcQueue = rpcQueue.then(fn, fn);
    return rpcQueue;
  }

  /**
   * Builds contiguous segments for a line WITHOUT any template whitespace between characters,
   * eliminating the wide letter-spacing bug caused by white-space: pre-wrap.
   */
  function buildLineSegments(lineText: string, rowIdx: number): RenderSegment[] {
    const chars = Array.from(lineText);
    const isActiveRow = rowIdx === nvimState.cursor_row;
    const cursorCol = nvimState.cursor_col_char;
    const isInsert = nvimState.mode.startsWith('i');
    const modelChars: StyledChar[] | undefined =
      activeTab.viewMode === 'rendered' ? activeTab.mdModel[rowIdx]?.chars : undefined;

    const segments: RenderSegment[] = [];

    function pushSeg(text: string, cls: string, isPreedit = false) {
      if (!text) return;
      const last = segments[segments.length - 1];
      if (last && !last.isPreedit && !isPreedit && last.cls === cls) {
        last.text += text;
      } else {
        segments.push({ text, cls, isPreedit });
      }
    }

    for (let colIdx = 0; colIdx <= chars.length; colIdx++) {
      const isCursor = isActiveRow && colIdx === cursorCol;

      if (isCursor && isComposing && preeditText) {
        pushSeg(preeditText, 'preedit', true);
      }

      if (colIdx < chars.length) {
        const mc = modelChars?.[colIdx];
        const classes: string[] = [];
        if (mc?.b) classes.push('bold-run');
        if (mc?.i) classes.push('italic-run');
        if (mc?.code) classes.push('code-run');
        if (isCursor) classes.push(isInsert ? 'cursor-bar' : 'cursor-block');
        pushSeg(chars[colIdx], classes.join(' '));
      } else if (isCursor) {
        pushSeg('\u00A0', isInsert ? 'cursor-bar' : 'cursor-block');
      }
    }

    if (segments.length === 0) {
      segments.push({ text: '\u00A0', cls: '' });
    }
    return segments;
  }

  async function loadTabIntoNvim(tab: TabDoc, row = 0, col = 0) {
    const targetLines =
      tab.viewMode === 'rendered'
        ? projectLines(tab.mdModel)
        : serializeMarkdown(tab.mdModel).split('\n');
    const st = await invoke<NvimState>('nvim_set_buffer', {
      lines: targetLines,
      cursorRow: row,
      cursorColChar: col,
    });
    nvimState = st;
    imeInputEl?.focus();
  }

  async function syncFromNvimState(state: NvimState) {
    const prevRow = nvimState.cursor_row;
    const prevMode = nvimState.mode;
    const tab = tabs[activeTabIndex];

    if (tab.viewMode === 'rendered') {
      let updatedModel = applyProjected(tab.mdModel, state.lines);
      if (!state.blocking) {
        const outcome = reparseCompletedLines(
          updatedModel,
          prevRow,
          prevMode,
          state.cursor_row,
          state.cursor_col_char,
          state.mode
        );
        updatedModel = outcome.model;
        if (outcome.bufferChanged) {
          state = await invoke<NvimState>('nvim_set_buffer', {
            lines: projectLines(updatedModel),
            cursorRow: state.cursor_row,
            cursorColChar: outcome.newCursorCol,
          });
        }
      }
      tab.mdModel = updatedModel;

      // If a yank operation occurred in rendered mode, overwrite register with source markdown
      if (state.last_yank) {
        const y = state.last_yank;
        const mdLines = extractMarkdownRange(
          tab.mdModel,
          y.start_row,
          y.start_col > 0 ? y.start_col - 1 : 0,
          y.end_row,
          y.end_col > 0 ? y.end_col - 1 : 0,
          y.regtype
        );
        if (mdLines.length > 0) {
          await invoke('nvim_set_register', {
            regname: y.regname,
            lines: mdLines,
            regtype: y.regtype,
          });
        }
      }
    } else {
      tab.mdModel = parseMarkdown(state.lines.join('\n'));
    }

    // If buffer content changed, mark tab as dirty (unsaved)
    if (state.lines.join('\n') !== nvimState.lines.join('\n')) {
      tab.dirty = true;
      scheduleDebouncedAutoSave(tab);
    }

    nvimState = state;

    if (state.ex_command) {
      await executeExCommand(state.ex_command);
    }
  }

  function sendNvimKeys(keys: string) {
    return enqueueRpc(async () => {
      const st = await invoke<NvimState>('nvim_input', { keys });
      await syncFromNvimState(st);
    });
  }

  function sendNvimPaste(text: string) {
    return enqueueRpc(async () => {
      const st = await invoke<NvimState>('nvim_paste_text', { text });
      await syncFromNvimState(st);
    });
  }

  async function switchViewMode(targetMode: 'rendered' | 'code') {
    const tab = tabs[activeTabIndex];
    if (tab.viewMode === targetMode) return;
    const fullMd = serializeMarkdown(tab.mdModel);
    tab.mdModel = parseMarkdown(fullMd);
    tab.viewMode = targetMode;
    await loadTabIntoNvim(tab, nvimState.cursor_row, nvimState.cursor_col_char);
  }

  function toggleSpanStyleUnderCursor(styleKey: 'b' | 'i' | 'code') {
    const tab = tabs[activeTabIndex];
    if (tab.viewMode !== 'rendered') return;
    const row = tab.mdModel[nvimState.cursor_row];
    if (!row || !row.chars[nvimState.cursor_col_char]) return;
    let s = nvimState.cursor_col_char;
    let end = s;
    while (s > 0 && row.chars[s - 1].c !== ' ') s--;
    while (end < row.chars.length && row.chars[end].c !== ' ') end++;
    const targetState = !row.chars[nvimState.cursor_col_char][styleKey];
    for (let k = s; k < end; k++) {
      row.chars[k][styleKey] = targetState;
    }
    row.dirty = true;
    tab.dirty = true;
    scheduleDebouncedAutoSave(tab);
    tab.mdModel = [...tab.mdModel];
  }

  function toggleBoldUnderCursor() {
    toggleSpanStyleUnderCursor('b');
  }

  function toggleItalicUnderCursor() {
    toggleSpanStyleUnderCursor('i');
  }

  function toggleCodeUnderCursor() {
    toggleSpanStyleUnderCursor('code');
  }

  function toggleCheckboxOnLine(rowIdx: number) {
    const tab = tabs[activeTabIndex];
    const line = tab.mdModel[rowIdx];
    if (!line) return;
    if (/^\s*[-*+] \[ \] $/.test(line.prefix)) {
      line.prefix = line.prefix.replace('[ ]', '[x]');
      line.dirty = true;
      tab.dirty = true;
      scheduleDebouncedAutoSave(tab);
    } else if (/^\s*[-*+] \[[xX]\] $/.test(line.prefix)) {
      line.prefix = line.prefix.replace(/\[[xX]\]/, '[ ]');
      line.dirty = true;
      tab.dirty = true;
      scheduleDebouncedAutoSave(tab);
    }
    tab.mdModel = [...tab.mdModel];
  }

  async function addNewTab() {
    const idx = tabs.length + 1;
    tabs.push({
      id: `tab-${Date.now()}`,
      title: `untitled-${idx}.md`,
      path: null,
      viewMode: 'rendered',
      mdModel: parseMarkdown(`# 새 문서 ${idx}\n\n내용을 입력하세요.`),
      dirty: false,
    });
    activeTabIndex = tabs.length - 1;
    await loadTabIntoNvim(tabs[activeTabIndex], 0, 0);
  }

  async function selectTab(idx: number) {
    if (idx < 0 || idx >= tabs.length || idx === activeTabIndex) return;
    const currentTab = tabs[activeTabIndex];
    if (currentTab?.path && currentTab.dirty) {
      await autoSaveTab(currentTab);
    }
    activeTabIndex = idx;
    await loadTabIntoNvim(tabs[activeTabIndex], 0, 0);
  }

  async function nextTab() {
    if (tabs.length <= 1) return;
    const nextIdx = (activeTabIndex + 1) % tabs.length;
    await selectTab(nextIdx);
  }

  async function prevTab() {
    if (tabs.length <= 1) return;
    const prevIdx = (activeTabIndex - 1 + tabs.length) % tabs.length;
    await selectTab(prevIdx);
  }

  async function closeActiveTab(force = false) {
    const tabToClose = tabs[activeTabIndex];
    if (!force && tabToClose?.path && tabToClose.dirty) {
      await autoSaveTab(tabToClose);
    }
    const timer = autoSaveTimers.get(tabToClose?.id);
    if (timer) {
      clearTimeout(timer);
      autoSaveTimers.delete(tabToClose.id);
    }

    if (tabs.length <= 1) {
      // Don't close the last tab, reset it to a clean untitled document instead
      tabs[0].title = 'untitled.md';
      tabs[0].path = null;
      tabs[0].mdModel = parseMarkdown('');
      tabs[0].dirty = false;
      await loadTabIntoNvim(tabs[0], 0, 0);
      return;
    }
    tabs.splice(activeTabIndex, 1);
    activeTabIndex = Math.min(activeTabIndex, tabs.length - 1);
    await loadTabIntoNvim(tabs[activeTabIndex], 0, 0);
  }

  async function executeExCommand(rawCmd: string) {
    const cmd = rawCmd.trim().replace(/^:+/, '').trim();
    if (
      cmd === 'w' ||
      cmd === 'w!' ||
      cmd === 'write' ||
      cmd === 'write!' ||
      cmd.startsWith('w ') ||
      cmd.startsWith('w! ') ||
      cmd.startsWith('write ') ||
      cmd.startsWith('write! ')
    ) {
      await saveFile();
    } else if (
      cmd === 'q!' ||
      cmd === 'quit!' ||
      cmd === 'bd!' ||
      cmd === 'bdelete!' ||
      cmd === 'bw!' ||
      cmd === 'bwipeout!' ||
      cmd === 'tabc!' ||
      cmd === 'tabclose!' ||
      cmd === 'close!' ||
      cmd === 'clo!' ||
      cmd === 'qa!' ||
      cmd === 'qall!' ||
      cmd === 'cq' ||
      cmd === 'cq!' ||
      cmd === 'cquit' ||
      cmd === 'cquit!'
    ) {
      await closeActiveTab(true);
    } else if (
      cmd === 'q' ||
      cmd === 'quit' ||
      cmd === 'bd' ||
      cmd === 'bdelete' ||
      cmd === 'bw' ||
      cmd === 'bwipeout' ||
      cmd === 'tabc' ||
      cmd === 'tabclose' ||
      cmd === 'close' ||
      cmd === 'clo' ||
      cmd === 'qa' ||
      cmd === 'qall'
    ) {
      await closeActiveTab(false);
    } else if (
      cmd === 'wq' ||
      cmd === 'wq!' ||
      cmd === 'wqa' ||
      cmd === 'wqa!' ||
      cmd === 'x' ||
      cmd === 'x!' ||
      cmd === 'xa' ||
      cmd === 'xa!' ||
      cmd === 'xit' ||
      cmd === 'xit!' ||
      cmd === 'exit' ||
      cmd === 'exit!'
    ) {
      const ok = await saveFile();
      if (ok || cmd.endsWith('!')) {
        await closeActiveTab(true);
      }
    } else if (cmd === 'tabnew' || cmd === 'tabe' || cmd === 'tabedit' || cmd === 'enew' || cmd === 'enew!') {
      await addNewTab();
    } else if (cmd === 'bn' || cmd === 'bnext' || cmd === 'tabn' || cmd === 'tabnext') {
      await nextTab();
    } else if (
      cmd === 'bp' ||
      cmd === 'bprev' ||
      cmd === 'bprevious' ||
      cmd === 'tabp' ||
      cmd === 'tabprev' ||
      cmd === 'tabprevious'
    ) {
      await prevTab();
    } else if (
      cmd === 'e' ||
      cmd === 'edit' ||
      cmd === 'e!' ||
      cmd === 'edit!' ||
      cmd.startsWith('e ') ||
      cmd.startsWith('edit ')
    ) {
      await openFile();
    }
  }

  async function openFile() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Markdown & Text', extensions: ['md', 'markdown', 'txt', 'text'] }],
      });
      if (!selected) return;
      const filePath = typeof selected === 'string' ? selected : (selected as any).path;
      if (!filePath) return;

      const content = await readTextFile(filePath);
      const fileName = filePath.split(/[/\\]/).pop() || 'document.md';

      // Check if current tab is an untouched untitled tab
      const current = tabs[activeTabIndex];
      const isCleanUntitled = current && !current.path && !current.dirty && current.title.startsWith('untitled');

      const newDoc: TabDoc = {
        id: `tab-${Date.now()}`,
        title: fileName,
        path: filePath,
        viewMode: current?.viewMode || 'rendered',
        mdModel: parseMarkdown(content),
        dirty: false,
      };

      if (isCleanUntitled) {
        tabs[activeTabIndex] = newDoc;
      } else {
        tabs.push(newDoc);
        activeTabIndex = tabs.length - 1;
      }

      await loadTabIntoNvim(tabs[activeTabIndex], 0, 0);
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  }

  async function saveFileAs(): Promise<boolean> {
    const tab = tabs[activeTabIndex];
    try {
      const filePath = await save({
        defaultPath: tab.path || tab.title || 'document.md',
        filters: [{ name: 'Markdown', extensions: ['md', 'markdown'] }],
      });
      if (!filePath) return false;

      const markdown = serializeMarkdown(tab.mdModel);
      await writeTextFile(filePath, markdown);

      tab.path = filePath;
      tab.title = filePath.split(/[/\\]/).pop() || tab.title;
      tab.dirty = false;
      return true;
    } catch (err) {
      console.error('Failed to save file as:', err);
      return false;
    }
  }

  let autoSaveTimers = new Map<string, ReturnType<typeof setTimeout>>();

  function scheduleDebouncedAutoSave(tab: TabDoc) {
    if (!tab.path) return;
    const existingTimer = autoSaveTimers.get(tab.id);
    if (existingTimer) {
      clearTimeout(existingTimer);
    }
    const timer = setTimeout(async () => {
      autoSaveTimers.delete(tab.id);
      if (tab.dirty && tab.path) {
        await autoSaveTab(tab);
      }
    }, config.autoSaveDebounceMs || 1500);
    autoSaveTimers.set(tab.id, timer);
  }

  async function autoSaveTab(tab: TabDoc): Promise<boolean> {
    if (!tab.path || !tab.dirty) return false;
    const existing = autoSaveTimers.get(tab.id);
    if (existing) {
      clearTimeout(existing);
      autoSaveTimers.delete(tab.id);
    }
    try {
      const markdown = serializeMarkdown(tab.mdModel);
      await writeTextFile(tab.path, markdown);
      tab.dirty = false;
      return true;
    } catch (err) {
      console.error(`Autosave failed for ${tab.path}:`, err);
      return false;
    }
  }

  async function autoSaveAllDirtyTabs(): Promise<void> {
    for (const tab of tabs) {
      if (tab.path && tab.dirty) {
        await autoSaveTab(tab);
      }
    }
  }

  async function saveFile(): Promise<boolean> {
    const tab = tabs[activeTabIndex];
    if (!tab.path) {
      return await saveFileAs();
    }
    try {
      const markdown = serializeMarkdown(tab.mdModel);
      await writeTextFile(tab.path, markdown);
      tab.dirty = false;
      return true;
    } catch (err) {
      console.error('Failed to save file:', err);
      return false;
    }
  }

  const commandRegistry = createDefaultRegistry({
    toggleViewMode: () => switchViewMode(activeTab.viewMode === 'rendered' ? 'code' : 'rendered'),
    switchViewMode: (m) => switchViewMode(m),
    toggleBold: () => toggleBoldUnderCursor(),
    toggleItalic: () => toggleItalicUnderCursor(),
    toggleCode: () => toggleCodeUnderCursor(),
    newTab: () => addNewTab(),
    nextTab: () => nextTab(),
    prevTab: () => prevTab(),
    closeActiveTab: () => closeActiveTab(),
    toggleInspector: () => { showInspector = !showInspector; },
    openFile: () => openFile(),
    saveFile: () => saveFile(),
    saveFileAs: () => saveFileAs(),
  });

  let commandContext = $derived<CommandContext>({
    activeTab: {
      id: activeTab.id,
      title: activeTab.title,
      viewMode: activeTab.viewMode,
      rawMarkdown: rawMarkdownPreview,
      dirty: activeTab.dirty,
    },
    cursor: {
      row: nvimState.cursor_row,
      col: nvimState.cursor_col_char,
    },
    mode: nvimState.mode,
    tabsCount: tabs.length,
    activeTabIndex,
  });

  async function handleExecuteCommand(cmd: CommandDefinition) {
    showCommandPalette = false;
    setTimeout(() => imeInputEl?.focus(), 50);
    try {
      await commandRegistry.execute(cmd.id, commandContext);
    } catch (err) {
      console.error('Failed to execute command:', err);
    }
  }

  let pendingEnterAfterComposition = false;

  function handleCompositionStart() {
    isComposing = true;
    preeditText = '';
  }

  function handleCompositionUpdate(e: CompositionEvent) {
    isComposing = true;
    preeditText = e.data || '';
  }

  function handleCompositionEnd(e: CompositionEvent) {
    const committed = e.data || '';
    const shouldSendEnter = pendingEnterAfterComposition;
    pendingEnterAfterComposition = false;
    isComposing = false;
    preeditText = '';
    if (committed && nvimState.mode.startsWith('i')) {
      sendNvimPaste(committed);
    }
    if (shouldSendEnter) {
      sendNvimKeys('<CR>');
    }
  }

  function buildKeyCombo(e: KeyboardEvent): string {
    const parts: string[] = [];
    if (e.ctrlKey || e.metaKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');

    if (e.key === 'F1' || e.key === 'F2' || e.key === 'F3' || e.key === 'F4' || e.key === 'F5' || e.key === 'F6' || e.key === 'F7' || e.key === 'F8' || e.key === 'F9' || e.key === 'F10' || e.key === 'F11' || e.key === 'F12') {
      parts.push(e.key);
    } else if (e.code.startsWith('Key')) {
      parts.push(e.code.slice(3).toUpperCase());
    } else if (e.key.length === 1 && /[a-zA-Z]/.test(e.key)) {
      parts.push(e.key.toUpperCase());
    } else if (e.key === 'Tab') {
      parts.push('Tab');
    }
    // Only return combo if at least one modifier is active or it is a function key
    if (parts.length > 1 || (parts.length === 1 && (parts[0].startsWith('F') || parts[0] === 'Tab'))) {
      return parts.join('+');
    }
    return '';
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (showCommandPalette) {
      return;
    }

    // Direct check for Ctrl+Shift+P / F1 before anything else
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === 'P' || e.key === 'p' || e.code === 'KeyP')) {
      e.preventDefault();
      showCommandPalette = true;
      return;
    }
    if (e.key === 'F1') {
      e.preventDefault();
      showCommandPalette = true;
      return;
    }

    const combo = buildKeyCombo(e);
    if (combo) {
      const target = resolveShortcut(config, combo, nvimState.mode);
      if (target === 'palette.open') {
        e.preventDefault();
        showCommandPalette = true;
        return;
      }
      if (target && target !== 'nvim') {
        e.preventDefault();
        commandRegistry.execute(target, commandContext).catch((err) => {
          console.error(`Failed to execute shortcut command ${target}:`, err);
        });
        return;
      }
    }

    if (e.key === 'Escape') {
      e.preventDefault();
      pendingEnterAfterComposition = false;
      if (isComposing && imeInputEl) {
        const pending = preeditText;
        isComposing = false;
        preeditText = '';
        imeInputEl.blur();
        imeInputEl.value = '';
        imeInputEl.focus();
        if (pending && nvimState.mode.startsWith('i')) {
          sendNvimPaste(pending);
        }
      }
      sendNvimKeys('<Esc>');
      return;
    }

    // If Enter is pressed while actively composing Hangul, remember to send <CR> right after compositionend
    if (preeditText && (e.key === 'Enter' || e.code === 'Enter')) {
      pendingEnterAfterComposition = true;
      return;
    }

    // In IBus-Hangul on WebKitGTK, Space or Enter when NOT actively composing Hangul
    // can still report keyCode === 229. Never swallow Space/Enter/Backspace when preeditText is empty!
    if (!preeditText && (e.code === 'Space' || e.key === ' ')) {
      e.preventDefault();
      isComposing = false;
      sendNvimKeys('<Space>');
      return;
    }

    if (!preeditText && (e.key === 'Enter' || e.code === 'Enter')) {
      e.preventDefault();
      isComposing = false;
      sendNvimKeys('<CR>');
      return;
    }

    if (!preeditText && e.key === 'Backspace') {
      e.preventDefault();
      isComposing = false;
      sendNvimKeys('<BS>');
      return;
    }

    if (
      isComposing ||
      e.isComposing ||
      e.keyCode === 229 ||
      e.key === 'Process' ||
      e.key === 'Unidentified'
    ) {
      return;
    }

    if (e.key.length === 1 && e.key.charCodeAt(0) > 127) {
      e.preventDefault();
      return;
    }

    e.preventDefault();

    const specialMap: Record<string, string> = {
      Backspace: '<BS>',
      Enter: '<CR>',
      Tab: '<Tab>',
      ' ': '<Space>',
      ArrowLeft: '<Left>',
      ArrowRight: '<Right>',
      ArrowUp: '<Up>',
      ArrowDown: '<Down>',
      Delete: '<Del>',
    };

    if (specialMap[e.key]) {
      sendNvimKeys(specialMap[e.key]);
      return;
    }

    if (e.ctrlKey && e.code.startsWith('Key')) {
      const letter = e.code[3].toLowerCase();
      sendNvimKeys(`<C-${letter}>`);
      return;
    }

    if (e.key.length === 1) {
      const k = e.key === '<' ? '<lt>' : e.key;
      sendNvimKeys(k);
    }
  }

  function computeLineClass(rowIdx: number, prefix: string | undefined, viewMode: string, cursorRow: number): string {
    let cls = 'line';
    if (rowIdx === cursorRow) cls += ' active-line';
    if (viewMode === 'rendered' && prefix) {
      if (isHorizontalRulePrefix(prefix)) {
        cls += ' hr-line';
        return cls;
      }
      const trimmed = prefix.trim();
      if (trimmed === '#') cls += ' h1';
      else if (trimmed === '##') cls += ' h2';
      else if (trimmed === '###') cls += ' h3';
      else if (/^#{4,6}$/.test(trimmed)) cls += ' h4';
      else if (/^\s*[-*+] \[[ xX]\] $/.test(prefix)) cls += ' task-item';
      else if (/^\s*[-*+] $/.test(prefix)) cls += ' ul';
      else if (/^\s*>+ $/.test(prefix)) cls += ' quote';
    }
    return cls;
  }

  onMount(async () => {
    // Try to load user configuration from ~/.config/mdvim/config.toml
    try {
      const tomlContent = await invoke<string | null>('app_load_config');
      if (tomlContent) {
        const parsed = parseTomlConfig(tomlContent);
        config = {
          ...defaultConfig,
          ...parsed,
          shortcuts: parsed.shortcuts && parsed.shortcuts.length > 0 ? parsed.shortcuts : defaultConfig.shortcuts,
        };
      }
    } catch (err) {
      console.warn('Failed to load user config.toml, using defaults:', err);
    }

    await loadTabIntoNvim(tabs[0], 0, 0);

    // Listen for external IPC control requests (mdvim ctl)
    const unlisten = await listen<{ id: number; method: string; params: any }>('ctl-request', async (event) => {
      const { id, method, params } = event.payload;
      try {
        if (method === 'list-commands') {
          const list = commandRegistry.list().map((c) => ({
            id: c.id,
            titleKo: c.titleKo,
            titleEn: c.titleEn,
            category: c.category,
            defaultShortcut: c.defaultShortcut,
          }));
          await invoke('ctl_response', { reqId: id, result: { status: 'ok', commands: list } });
        } else if (method === 'exec') {
          const cmdId = typeof params === 'string' ? params : params?.id;
          const args = typeof params === 'object' ? params?.args : undefined;
          const res = await commandRegistry.execute(cmdId, commandContext, args);
          await invoke('ctl_response', { reqId: id, result: { status: 'ok', result: res ?? null } });
        } else if (method === 'send-keys') {
          const keys = typeof params === 'string' ? params : params?.keys || '';
          await sendNvimKeys(keys);
          await invoke('ctl_response', { reqId: id, result: { status: 'ok' } });
        } else if (method === 'insert-text') {
          const text = typeof params === 'string' ? params : params?.text || '';
          await sendNvimPaste(text);
          await invoke('ctl_response', { reqId: id, result: { status: 'ok' } });
        } else {
          await invoke('ctl_response', {
            reqId: id,
            result: { status: 'error', message: `Unknown method: ${method}` },
          });
        }
      } catch (err: any) {
        await invoke('ctl_response', {
          reqId: id,
          result: { status: 'error', message: err?.message || String(err) },
        });
      }
    });

    // Setup window blur listener for auto-save
    const onWindowBlur = () => {
      if (config.autoSaveOnBlur) {
        autoSaveAllDirtyTabs().catch((err) => console.error('Blur autosave error:', err));
      }
    };
    window.addEventListener('blur', onWindowBlur);

    // Setup periodic interval autosave (e.g. every 30s)
    let intervalTimer: ReturnType<typeof setInterval> | undefined;
    if (config.autoSaveIntervalMs && config.autoSaveIntervalMs > 0) {
      intervalTimer = setInterval(() => {
        autoSaveAllDirtyTabs().catch((err) => console.error('Interval autosave error:', err));
      }, config.autoSaveIntervalMs);
    }

    return () => {
      unlisten();
      window.removeEventListener('blur', onWindowBlur);
      if (intervalTimer) clearInterval(intervalTimer);
      for (const t of autoSaveTimers.values()) {
        clearTimeout(t);
      }
      autoSaveTimers.clear();
    };
  });

  // Continuously report current state to backend IPC cache
  $effect(() => {
    const st = {
      activeTab: {
        id: activeTab.id,
        title: activeTab.title,
        path: activeTab.path,
        viewMode: activeTab.viewMode,
        rawMarkdown: rawMarkdownPreview,
        dirty: activeTab.dirty,
      },
      tabs: tabs.map((t) => ({
        id: t.id,
        title: t.title,
        path: t.path,
        viewMode: t.viewMode,
        dirty: t.dirty,
      })),
      activeTabIndex,
      cursor: {
        row: nvimState.cursor_row,
        col: nvimState.cursor_col_char,
      },
      mode: nvimState.mode,
      blocking: nvimState.blocking,
    };
    invoke('ctl_report_state', { stateJson: st }).catch(() => {});
  });
</script>

<div class="app-shell">
  <header class="topbar">
    <div class="tabs-bar">
      {#each tabs as tab, idx (tab.id)}
        <button
          class="tab-btn {idx === activeTabIndex ? 'active' : ''}"
          onclick={() => selectTab(idx)}
        >
          {tab.title}{tab.dirty ? ' •' : ''}
        </button>
      {/each}
      <button class="tab-add-btn" onclick={addNewTab}>{msg.newTab}</button>
    </div>

    <div class="view-switcher">
      <button
        class="action-btn"
        onclick={openFile}
        title="파일 열기 (Ctrl+O)"
      >
        📂 {msg.openFile}
      </button>
      <button
        class="action-btn"
        onclick={saveFile}
        title="파일 저장 (Ctrl+S)"
      >
        💾 {msg.saveFile}
      </button>
      <button
        class={activeTab.viewMode === 'rendered' ? 'active' : ''}
        onclick={() => switchViewMode('rendered')}
      >
        {msg.renderedView}
      </button>
      <button
        class={activeTab.viewMode === 'code' ? 'active' : ''}
        onclick={() => switchViewMode('code')}
      >
        {msg.codeView}
      </button>
      <button
        class={showInspector ? 'active' : ''}
        onclick={() => (showInspector = !showInspector)}
        title="실제 마크다운 원본 확인 패널 토글"
      >
        MD 원본 확인
      </button>
      <button
        class="lang-btn"
        onclick={() => (config.locale = config.locale === 'ko' ? 'en' : 'ko')}
      >
        {config.locale.toUpperCase()}
      </button>
      <button
        class="palette-btn"
        onclick={() => (showCommandPalette = true)}
        title="커맨드 팔레트 (Ctrl+Shift+P / F1)"
      >
        ⌨ 커맨드 (Ctrl+Shift+P)
      </button>
    </div>
  </header>

  <div class="workspace {showInspector ? 'with-inspector' : ''}">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <main
      class="editor-pane {activeTab.viewMode}"
      onmousedown={() => setTimeout(() => imeInputEl?.focus(), 0)}
    >
      <textarea
        bind:this={imeInputEl}
        class="ime-input"
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
        spellcheck="false"
        oncompositionstart={handleCompositionStart}
        oncompositionupdate={handleCompositionUpdate}
        oncompositionend={handleCompositionEnd}
        onkeydown={handleKeyDown}
      ></textarea>

      <div class="lines-container">
        {#each nvimState.lines as lineText, rowIdx}
          {@const modelLine = activeTab.mdModel[rowIdx]}
          {@const isHr = activeTab.viewMode === 'rendered' && modelLine && isHorizontalRulePrefix(modelLine.prefix) && lineText.length === 0}
          {@const segments = buildLineSegments(lineText, rowIdx)}
          <div class={computeLineClass(rowIdx, modelLine?.prefix, activeTab.viewMode, nvimState.cursor_row)}>{#if isHr}<span class="hr-bar {rowIdx === nvimState.cursor_row ? 'hr-active' : ''}"></span>{:else}{#if activeTab.viewMode === 'rendered' && modelLine && /^\s*[-*+] \[[ xX]\] $/.test(modelLine.prefix)}<input
                type="checkbox"
                class="task-checkbox"
                checked={/\[[xX]\]/.test(modelLine.prefix)}
                onchange={() => toggleCheckboxOnLine(rowIdx)}
              />{/if}{#each segments as seg}{#if seg.cls}<span class={seg.cls}>{seg.text}</span>{:else}{seg.text}{/if}{/each}{/if}</div>
        {/each}
      </div>
    </main>

    {#if showInspector}
      <aside class="inspector-pane">
        <div class="inspector-title">실제 마크다운 원본 (실시간 역변환)</div>
        <pre class="raw-md">{rawMarkdownPreview}</pre>
      </aside>
    {/if}
  </div>

  <footer class="statusline">
    <div class="status-left">
      <span
        class="mode-badge {nvimState.mode.startsWith('i')
          ? 'mode-i'
          : nvimState.mode.startsWith('v') || nvimState.mode.startsWith('V')
            ? 'mode-v'
            : nvimState.mode === 'c'
              ? 'mode-c'
              : 'mode-n'}"
      >
        {nvimState.mode.startsWith('i')
          ? msg.insertMode
          : nvimState.mode.startsWith('v') || nvimState.mode.startsWith('V')
            ? msg.visualMode
            : nvimState.mode === 'c'
              ? msg.commandMode
              : msg.normalMode}
      </span>
      {#if nvimState.mode === 'c'}
        <span class="cmdline-display">:{nvimState.cmdline || ''}</span>
      {:else}
        <span class="cursor-pos">
          {msg.lineCol(nvimState.cursor_row + 1, nvimState.cursor_col_char + 1)}
        </span>
      {/if}
      {#if nvimState.blocking}
        <span class="waiting-badge">[{msg.waitingKey}]</span>
      {/if}
      {#if isComposing && preeditText}
        <span class="preedit-badge">{msg.composing}: "{preeditText}"</span>
      {/if}
    </div>

    <div class="status-right">
      <span>{activeTab.title}</span>
      <span>•</span>
      <span>{activeTab.viewMode === 'rendered' ? msg.renderedView : msg.codeView}</span>
    </div>
  </footer>

  {#if showCommandPalette}
    <CommandPalette
      commands={commandRegistry.list()}
      context={commandContext}
      locale={config.locale}
      onClose={() => {
        showCommandPalette = false;
        setTimeout(() => imeInputEl?.focus(), 50);
      }}
      onExecute={handleExecuteCommand}
    />
  {/if}
</div>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg-primary);
    color: var(--text-primary);
  }
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-color);
    gap: 12px;
  }
  .tabs-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    overflow-x: auto;
  }
  .tab-btn,
  .tab-add-btn,
  .view-switcher button {
    background: #ffffff;
    color: var(--text-primary);
    border: 1px solid #d4d4d8;
    border-radius: 6px;
    padding: 5px 10px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .tab-btn.active,
  .view-switcher button.active {
    background: var(--accent-color);
    color: #ffffff;
    border-color: var(--accent-hover);
  }
  .view-switcher {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .palette-btn {
    background: #f1f5f9;
    border-color: #cbd5e1;
    font-weight: 600;
    color: #334155;
  }
  .palette-btn:hover {
    background: #e2e8f0;
    color: #0f172a;
  }
  .workspace {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr;
    min-height: 0;
  }
  .workspace.with-inspector {
    grid-template-columns: 1fr 340px;
  }
  .editor-pane {
    padding: 24px 36px;
    overflow-y: auto;
    cursor: text;
    position: relative;
  }
  .inspector-pane {
    border-left: 1px solid var(--border-color);
    background: #fafafa;
    padding: 12px;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .inspector-title {
    font-size: 11px;
    font-weight: 700;
    color: var(--text-secondary);
    text-transform: uppercase;
    margin-bottom: 8px;
  }
  .raw-md {
    margin: 0;
    flex: 1;
    font-family: 'JetBrains Mono', 'D2Coding', monospace;
    font-size: 12px;
    line-height: 1.5;
    color: #15803d;
    background: #ffffff;
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 10px;
    white-space: pre-wrap;
    overflow-y: auto;
  }
  .ime-input {
    position: fixed;
    top: 0;
    left: 0;
    opacity: 0;
    pointer-events: none;
    width: 4px;
    height: 16px;
    z-index: -1;
  }
  .line {
    min-height: 1.65em;
    line-height: 1.65;
    white-space: pre-wrap;
    word-break: break-all;
    padding: 1px 6px;
    border-radius: 4px;
    font-family: 'JetBrains Mono', 'D2Coding', monospace;
    font-size: 15px;
    letter-spacing: normal;
  }
  .line.active-line {
    background: #f4f4f5;
  }
  .rendered .line.h1 {
    font-size: 24px;
    font-weight: 700;
    color: #1d4ed8;
    border-bottom: 1px solid var(--border-color);
    padding-bottom: 4px;
    margin-bottom: 6px;
  }
  .rendered .line.h2 {
    font-size: 20px;
    font-weight: 700;
    color: #1e40af;
  }
  .rendered .line.h3 {
    font-size: 17px;
    font-weight: 700;
    color: #1e3a8a;
  }
  .rendered .line.ul::before {
    content: '• ';
    color: var(--accent-color);
    font-weight: bold;
  }
  .rendered .line.quote {
    border-left: 3px solid #93c5fd;
    padding-left: 12px;
    color: #475569;
  }
  .rendered .line.hr-line {
    display: flex;
    align-items: center;
    min-height: 1.65em;
    padding: 4px 6px;
  }
  .hr-bar {
    display: block;
    width: 100%;
    height: 2px;
    background: #cbd5e1;
    border-radius: 2px;
  }
  .hr-bar.hr-active {
    background: var(--accent-color);
    height: 3px;
  }
  .task-checkbox {
    margin-right: 6px;
    cursor: pointer;
    vertical-align: middle;
  }
  .bold-run {
    font-weight: 700;
    color: var(--bold-color);
    background: var(--bold-bg);
    border-radius: 2px;
  }
  .italic-run {
    font-style: italic;
    color: #4338ca;
  }
  .code-run {
    font-family: 'JetBrains Mono', 'D2Coding', monospace;
    background: #f1f5f9;
    color: #be123c;
    padding: 1px 4px;
    border-radius: 4px;
    font-size: 0.92em;
  }
  .cursor-block {
    background: var(--cursor-block-bg);
    color: var(--cursor-block-fg);
    border-radius: 2px;
  }
  .cursor-bar {
    border-left: 2px solid var(--cursor-bar-color);
    margin-left: -1px;
  }
  .preedit {
    background: var(--preedit-bg);
    color: var(--preedit-fg);
    border-bottom: 2px dashed var(--preedit-border);
    padding: 0 2px;
    border-radius: 2px;
  }
  .statusline {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 14px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border-color);
    font-family: monospace;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .status-left,
  .status-right {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .mode-badge {
    padding: 2px 8px;
    border-radius: 4px;
    font-weight: 700;
    color: #ffffff;
  }
  .mode-n {
    background: #2563eb;
  }
  .mode-i {
    background: #ea580c;
  }
  .mode-v {
    background: #9333ea;
  }
  .mode-c {
    background: #059669;
  }
  .cmdline-display {
    font-weight: 700;
    color: #0f172a;
    background: #e2e8f0;
    padding: 2px 6px;
    border-radius: 4px;
    font-family: 'JetBrains Mono', 'D2Coding', monospace;
  }
  .waiting-badge {
    color: #b91c1c;
    font-weight: 600;
  }
  .preedit-badge {
    color: #c2410c;
  }
</style>
