<script lang="ts">
  // Browses folders on the server (mini PC) to pick a library root.
  import { api, ApiError, type FsListing } from '../lib/api';
  import { tr, errorText } from '../lib/i18n/tr';
  import { IconArrowUp, IconDatabase, IconFolder, IconServer2 } from '../lib/icons';

  let { onAdded, onCancel }: { onAdded: () => void; onCancel: () => void } = $props();

  const l = tr.libraries;
  let listing = $state<FsListing | null>(null);
  let name = $state('');
  let error = $state('');
  let busy = $state(false);

  async function open(path: string | null) {
    error = '';
    try {
      listing = await api.get<FsListing>(`/admin/fs${path ? `?path=${encodeURIComponent(path)}` : ''}`);
      const last = (listing.path ?? '').replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? '';
      name = last.includes(':') ? '' : last;
    } catch (e) {
      error = errorText(e instanceof ApiError ? e.code : 'internal');
    }
  }
  open(null);

  async function add() {
    if (!listing?.path) return;
    busy = true;
    error = '';
    try {
      await api.post('/admin/libraries', { path: listing.path, name, scan: true });
      onAdded();
    } catch (e) {
      error = errorText(e instanceof ApiError ? e.code : 'internal');
    } finally {
      busy = false;
    }
  }
</script>

<div class="card picker">
  <div class="head">
    <h2>{l.pickerTitle}</h2>
    <button class="btn sm ghost" onclick={onCancel}>{tr.common.close}</button>
  </div>
  <p class="muted hint">{l.pickerHint}</p>

  <div class="nav">
    <button class="btn sm" onclick={() => open(null)}><IconServer2 size={15} />{l.drives}</button>
    <button class="btn sm" disabled={!listing?.path} onclick={() => open(listing?.parent || null)}>
      <IconArrowUp size={15} />{l.up}
    </button>
    <code class="path">{listing?.path ?? l.drives}</code>
  </div>

  <div class="list">
    {#if listing}
      {#each listing.entries as entry (entry.path)}
        <button class="entry" onclick={() => open(entry.path)}>
          {#if listing.path}<IconFolder size={17} class="entry-icon" />{:else}<IconServer2 size={17} class="entry-icon" />{/if}
          <span class="name">{entry.name}</span>
          {#if entry.is_library}<span class="badge accent lib"><IconDatabase size={12} />{l.isLibrary}</span>{/if}
        </button>
      {:else}
        <div class="muted none">{l.noSubfolders}</div>
      {/each}
    {/if}
  </div>

  {#if error}<div class="alert error">{error}</div>{/if}

  {#if listing?.path}
    <div class="confirm">
      <label class="field grow">
        <span>{l.libraryName}</span>
        <input class="input" bind:value={name} placeholder="Aile Diski" />
      </label>
      <button class="btn primary" disabled={busy} onclick={add}>{l.addAndScan}</button>
    </div>
  {/if}
</div>

<style>
  .picker {
    padding: 14px 16px;
    display: grid;
    gap: 10px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .hint {
    margin: 0;
    font-size: 12px;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .path {
    flex: 1;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .list {
    max-height: 260px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: none;
    border-bottom: 1px solid var(--border);
    background: transparent;
    cursor: pointer;
    text-align: left;
  }
  .entry:last-child {
    border-bottom: none;
  }
  .entry:hover {
    background: var(--surface-2);
  }
  .name {
    flex: 1;
  }
  .entry :global(.entry-icon) {
    color: var(--warning);
    flex: none;
  }
  .lib {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .none {
    padding: 10px;
    font-size: 13px;
  }
  .confirm {
    display: flex;
    align-items: flex-end;
    gap: 10px;
  }
  .grow {
    flex: 1;
  }
</style>
