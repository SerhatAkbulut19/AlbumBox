<script lang="ts">
  import type { Snippet } from 'svelte';
  import { IconX } from './icons';
  import { tr } from './i18n/tr';

  let {
    title,
    width = 440,
    onClose,
    children,
    footer,
  }: { title: string; width?: number; onClose: () => void; children: Snippet; footer?: Snippet } = $props();

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="card dialog" style:width={`${width}px`} role="dialog" aria-label={title}>
    <header>
      <h2>{title}</h2>
      <button class="btn sm ghost" onclick={onClose} aria-label={tr.common.close}><IconX size={16} /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(20, 24, 32, 0.35);
    display: grid;
    place-items: center;
    z-index: 50;
    padding: 20px;
  }
  .dialog {
    max-width: 100%;
    max-height: calc(100vh - 40px);
    display: grid;
    grid-template-rows: auto 1fr auto;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }
  .body {
    padding: 14px 16px;
    overflow: auto;
    display: grid;
    gap: 12px;
    align-content: start;
  }
  footer {
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
