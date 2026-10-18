<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { defaultConfig, resolveShortcut } from './lib/config';
  import { t } from './lib/i18n';
  import {
    parseMarkdown,
    projectLines,
    serializeMarkdown,
    applyProjected,
    reparseCompletedLines,
    type ProjectedLine,
  } from './lib/projection';

  interface NvimState {
    lines: string[];
    cursor_row: number;
    cursor_col_byte: number;
    cursor_col_char: number;
    mode: string;
    blocking: boolean;
  }

  interface TabDoc {
    id: string;
    title: string;
    path: string | null;
    viewMode: 'rendered' | 'code';
    mdModel: ProjectedLine[];
    dirty: boolean;
  }

  let config = $state({ ...defaultConfig });
  let msg = $derived(t(config.locale));

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
          '렌더링 뷰에서 **굵은 글씨**와 Vim 모션을 바로 사용할 수 있습니다.',
          '- 첫 번째 목록 항목',
          '- [ ] 할 일 체크박스 항목',
          '',
          '빈 줄에서 `# 제목` 또는 `**강조**`를 입력하고 Enter나 Esc를 눌러 보세요!',
        ].join('\n')
      ),
      dirty: false,
    },
  ]);
  let activeTabIndex = $state(0);
  let activeTab = $derived(tabs[activeTabIndex]);

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
    } else {
      tab.mdModel = parseMarkdown(state.lines.join('\n'));
    }

    nvimState = state;
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

  function toggleBoldUnderCursor() {
    const tab = tabs[activeTabIndex];
    if (tab.viewMode !== 'rendered') return;
    const row = tab.mdModel[nvimState.cursor_row];
    if (!row || !row.chars[nvimState.cursor_col_char]) return;
    let s = nvimState.cursor_col_char;
    let end = s;
    while (s > 0 && row.chars[s - 1].c !== ' ') s--;
    while (end < row.chars.length && row.chars[end].c !== ' ') end++;
    const targetBold = !row.chars[nvimState.cursor_col_char].b;
    for (let k = s; k < end; k++) row.chars[k].b = targetBold;
    row.dirty = true;
  }

  function toggleCheckboxOnLine(rowIdx: number) {
    const tab = tabs[activeTabIndex];
    const line = tab.mdModel[rowIdx];
    if (!line) return;
    if (/^\s*[-*+] \[ \] $/.test(line.prefix)) {
      line.prefix = line.prefix.replace('[ ]', '[x]');
      line.dirty = true;
    } else if (/^\s*[-*+] \[[xX]\] $/.test(line.prefix)) {
      line.prefix = line.prefix.replace(/\[[xX]\]/, '[ ]');
      line.dirty = true;
    }
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
    if (idx === activeTabIndex) return;
    activeTabIndex = idx;
    await loadTabIntoNvim(tabs[activeTabIndex], 0, 0);
  }

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
    isComposing = false;
    preeditText = '';
    if (committed && nvimState.mode.startsWith('i')) {
      sendNvimPaste(committed);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.code.startsWith('Key')) {
      const letter = e.code[3].toUpperCase();
      const combo = `Ctrl+${letter}`;
      const target = resolveShortcut(config, combo, nvimState.mode);
      if (target === 'app:toggle-bold') {
        e.preventDefault();
        toggleBoldUnderCursor();
        return;
      }
      if (target === 'app:toggle-View') {
        e.preventDefault();
        switchViewMode(activeTab.viewMode === 'rendered' ? 'code' : 'rendered');
        return;
      }
    }

    if (e.key === 'Escape') {
      e.preventDefault();
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

  function lineClass(rowIdx: number): string {
    let cls = 'line';
    if (rowIdx === nvimState.cursor_row) cls += ' active-line';
    if (activeTab.viewMode === 'rendered' && activeTab.mdModel[rowIdx]) {
      const pfx = activeTab.mdModel[rowIdx].prefix;
      if (pfx === '# ') cls += ' h1';
      else if (pfx === '## ') cls += ' h2';
      else if (pfx === '### ') cls += ' h3';
      else if (/^\s*[-*+] \[[ xX]\] $/.test(pfx)) cls += ' task-item';
      else if (/^\s*[-*+] $/.test(pfx)) cls += ' ul';
      else if (/^\s*>+ $/.test(pfx)) cls += ' quote';
    }
    return cls;
  }

  onMount(async () => {
    await loadTabIntoNvim(tabs[0], 0, 0);
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
          {tab.title}
        </button>
      {/each}
      <button class="tab-add-btn" onclick={addNewTab}>{msg.newTab}</button>
    </div>

    <div class="view-switcher">
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
        class="lang-btn"
        onclick={() => (config.locale = config.locale === 'ko' ? 'en' : 'ko')}
      >
        {config.locale.toUpperCase()}
      </button>
    </div>
  </header>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
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
        {@const chars = Array.from(lineText)}
        {@const modelLine = activeTab.mdModel[rowIdx]}
        <div class={lineClass(rowIdx)}>
          {#if activeTab.viewMode === 'rendered' && modelLine && /^\s*[-*+] \[[ xX]\] $/.test(modelLine.prefix)}
            <input
              type="checkbox"
              class="task-checkbox"
              checked={/\[[xX]\]/.test(modelLine.prefix)}
              onchange={() => toggleCheckboxOnLine(rowIdx)}
            />
          {/if}

          {#each { length: chars.length + 1 } as _, colIdx}
            {@const isCursor = rowIdx === nvimState.cursor_row && colIdx === nvimState.cursor_col_char}
            {@const isInsert = nvimState.mode.startsWith('i')}
            {@const isBold = activeTab.viewMode === 'rendered' && modelLine?.chars[colIdx]?.b}

            {#if isCursor && isComposing && preeditText}
              <span class="preedit">{preeditText}</span>
            {/if}

            {#if colIdx < chars.length}
              <span
                class="{isBold ? 'bold-run ' : ''}{isCursor
                  ? isInsert
                    ? 'cursor-bar'
                    : 'cursor-block'
                  : ''}">{chars[colIdx]}</span
              >
            {:else if isCursor}
              <span class={isInsert ? 'cursor-bar' : 'cursor-block'}>&nbsp;</span>
            {/if}
          {/each}
        </div>
      {/each}
    </div>
  </main>

  <footer class="statusline">
    <div class="status-left">
      <span
        class="mode-badge {nvimState.mode.startsWith('i')
          ? 'mode-i'
          : nvimState.mode.startsWith('v') || nvimState.mode.startsWith('V')
            ? 'mode-v'
            : 'mode-n'}"
      >
        {nvimState.mode.startsWith('i')
          ? msg.insertMode
          : nvimState.mode.startsWith('v') || nvimState.mode.startsWith('V')
            ? msg.visualMode
            : msg.normalMode}
      </span>
      <span class="cursor-pos">
        {msg.lineCol(nvimState.cursor_row + 1, nvimState.cursor_col_char + 1)}
      </span>
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
  .editor-pane {
    flex: 1;
    padding: 24px 36px;
    overflow-y: auto;
    cursor: text;
    position: relative;
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
    min-height: 1.7em;
    line-height: 1.7;
    white-space: pre-wrap;
    word-break: break-all;
    padding: 1px 6px;
    border-radius: 4px;
    font-family: 'JetBrains Mono', 'D2Coding', monospace;
    font-size: 15px;
  }
  .line.active-line {
    background: var(--bg-active-line);
  }
  .rendered .line.h1 {
    font-size: 26px;
    font-weight: 700;
    color: #1d4ed8;
    border-bottom: 1px solid var(--border-color);
    padding-bottom: 4px;
    margin-bottom: 6px;
  }
  .rendered .line.h2 {
    font-size: 21px;
    font-weight: 700;
    color: #1e40af;
  }
  .rendered .line.h3 {
    font-size: 18px;
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
  .task-checkbox {
    margin-right: 6px;
    cursor: pointer;
  }
  .bold-run {
    font-weight: 700;
    color: var(--bold-color);
    background: var(--bold-bg);
    border-radius: 2px;
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
  .waiting-badge {
    color: #b91c1c;
    font-weight: 600;
  }
  .preedit-badge {
    color: #c2410c;
  }
</style>
