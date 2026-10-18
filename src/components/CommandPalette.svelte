<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { CommandDefinition, CommandContext } from '../lib/commands';

  interface Props {
    commands: CommandDefinition[];
    context: CommandContext;
    locale: 'ko' | 'en';
    onClose: () => void;
    onExecute: (cmd: CommandDefinition) => void;
  }

  let { commands, context, locale, onClose, onExecute }: Props = $props();

  let query = $state('');
  let selectedIndex = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter((cmd) => {
      const title = locale === 'ko' ? cmd.titleKo : cmd.titleEn;
      const altTitle = locale === 'ko' ? cmd.titleEn : cmd.titleKo;
      return (
        cmd.id.toLowerCase().includes(q) ||
        title.toLowerCase().includes(q) ||
        altTitle.toLowerCase().includes(q) ||
        cmd.category.toLowerCase().includes(q)
      );
    });
  });

  $effect(() => {
    // Whenever query changes, reset selected index
    if (query !== undefined) {
      selectedIndex = 0;
    }
  });

  onMount(() => {
    tick().then(() => inputEl?.focus());
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
      return;
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (filtered.length > 0) {
        selectedIndex = (selectedIndex + 1) % filtered.length;
      }
      return;
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (filtered.length > 0) {
        selectedIndex = (selectedIndex - 1 + filtered.length) % filtered.length;
      }
      return;
    }

    if (e.key === 'Enter') {
      e.preventDefault();
      const target = filtered[selectedIndex];
      if (target) {
        onExecute(target);
      }
    }
  }

  function handleSelect(cmd: CommandDefinition) {
    onExecute(cmd);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="palette-backdrop" onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
  <div class="palette-modal">
    <div class="palette-header">
      <input
        bind:this={inputEl}
        bind:value={query}
        class="palette-input"
        type="text"
        placeholder={locale === 'ko' ? '명령어 검색 (예: 뷰, 탭, 굵게, view, tab...)' : 'Type a command to run...'}
        onkeydown={handleKeyDown}
      />
    </div>

    <div class="palette-list">
      {#if filtered.length === 0}
        <div class="palette-empty">
          {locale === 'ko' ? '일치하는 명령어가 없습니다.' : 'No matching commands found.'}
        </div>
      {:else}
        {#each filtered as cmd, idx (cmd.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div
            class="palette-item {idx === selectedIndex ? 'selected' : ''}"
            onclick={() => handleSelect(cmd)}
            onmouseenter={() => (selectedIndex = idx)}
          >
            <div class="item-left">
              <span class="category-tag">{cmd.category}</span>
              <span class="cmd-title">{locale === 'ko' ? cmd.titleKo : cmd.titleEn}</span>
              <span class="cmd-id">({cmd.id})</span>
            </div>
            {#if cmd.defaultShortcut}
              <kbd class="shortcut-badge">{cmd.defaultShortcut}</kbd>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

    <div class="palette-footer">
      <span>↑↓ {locale === 'ko' ? '이동' : 'Navigate'}</span>
      <span>↵ {locale === 'ko' ? '실행' : 'Run'}</span>
      <span>Esc {locale === 'ko' ? '닫기' : 'Dismiss'}</span>
    </div>
  </div>
</div>

<style>
  .palette-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(15, 23, 42, 0.4);
    backdrop-filter: blur(2px);
    z-index: 1000;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
  }

  .palette-modal {
    width: 620px;
    max-width: 90vw;
    background: #ffffff;
    border: 1px solid #cbd5e1;
    border-radius: 10px;
    box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.15), 0 8px 10px -6px rgba(0, 0, 0, 0.1);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .palette-header {
    border-bottom: 1px solid #e2e8f0;
    padding: 10px 14px;
    background: #f8fafc;
  }

  .palette-input {
    width: 100%;
    padding: 8px 10px;
    font-size: 14px;
    font-family: inherit;
    border: 1px solid #cbd5e1;
    border-radius: 6px;
    outline: none;
    background: #ffffff;
    color: #0f172a;
    box-sizing: border-box;
  }

  .palette-input:focus {
    border-color: #3b82f6;
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.2);
  }

  .palette-list {
    max-height: 340px;
    overflow-y: auto;
    padding: 6px 0;
  }

  .palette-empty {
    padding: 24px;
    text-align: center;
    color: #64748b;
    font-size: 13px;
  }

  .palette-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 14px;
    cursor: pointer;
    font-size: 13px;
    transition: background 0.1s ease;
  }

  .palette-item.selected {
    background: #eff6ff;
  }

  .item-left {
    display: flex;
    align-items: center;
    gap: 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .category-tag {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    background: #e2e8f0;
    color: #475569;
    padding: 2px 6px;
    border-radius: 4px;
    letter-spacing: 0.3px;
  }

  .cmd-title {
    color: #1e293b;
    font-weight: 500;
  }

  .cmd-id {
    color: #94a3b8;
    font-size: 11px;
    font-family: ui-monospace, monospace;
  }

  .shortcut-badge {
    background: #f1f5f9;
    border: 1px solid #cbd5e1;
    border-radius: 4px;
    padding: 2px 6px;
    font-size: 11px;
    color: #475569;
    font-family: ui-monospace, monospace;
    flex-shrink: 0;
  }

  .palette-footer {
    display: flex;
    gap: 16px;
    padding: 8px 14px;
    border-top: 1px solid #e2e8f0;
    background: #f8fafc;
    font-size: 11px;
    color: #64748b;
  }
</style>
