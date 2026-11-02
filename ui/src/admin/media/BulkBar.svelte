<script lang="ts">
  // Actions for the selected media: tags, category copy/move, publishing,
  // date, hide. Each opens a small panel above the bar.
  import type { SvelteSet } from 'svelte/reactivity';
  import { ApiError, bulk, type BulkOp, type Tag } from '../../lib/api';
  import { catalog } from '../../lib/catalog.svelte';
  import { tr, errorText } from '../../lib/i18n/tr';
  import {
    IconArrowsMove,
    IconCalendar,
    IconCopy,
    IconDeselect,
    IconEye,
    IconEyeOff,
    IconFolder,
    IconSelectAll,
    IconShare,
    IconTag,
    IconTagOff,
    IconX,
  } from '../../lib/icons';
  import TagInput from '../../lib/TagInput.svelte';
  import VisibilityPicker from '../../lib/VisibilityPicker.svelte';

  let {
    selection,
    filterCategory,
    onSelectAll,
    onDone,
  }: {
    selection: SvelteSet<number>;
    filterCategory: number | null;
    onSelectAll: () => void;
    onDone: (message: string) => void;
  } = $props();

  const b = tr.bulk;
  type Panel = 'tag-add' | 'tag-remove' | 'category' | 'visibility' | 'date' | null;
  let panel = $state<Panel>(null);
  let busy = $state(false);
  let error = $state('');
  let categoryId = $state('');
  let visMode = $state<'inherit' | 'private' | 'profiles'>('inherit');
  let visProfiles = $state<number[]>([]);
  let date = $state('');

  function toggle(p: Panel) {
    panel = panel === p ? null : p;
    error = '';
  }

  async function run(op: BulkOp, close = true) {
    busy = true;
    error = '';
    try {
      const r = await bulk([...selection], op);
      onDone(b.done(r.changed));
      if (close) panel = null;
      if (op.action.includes('categor')) catalog.reloadCategories();
      if (op.action === 'set_visibility') catalog.reloadProfiles();
    } catch (e) {
      error = errorText(e instanceof ApiError ? e.code : 'internal');
    } finally {
      busy = false;
    }
  }

  const cat = () => Number(categoryId);
</script>

<div class="bar-wrap">
  {#if panel}
    <div class="card panel">
      <button class="btn sm ghost close" onclick={() => (panel = null)} aria-label={tr.common.close}><IconX size={15} /></button>
      {#if panel === 'tag-add'}
        <strong>{b.addTag}</strong>
        <TagInput allowCreate onPick={(t: Tag) => run({ action: 'add_tags', tag_ids: [t.id] }, false)} />
      {:else if panel === 'tag-remove'}
        <strong>{b.removeTag}</strong>
        <TagInput onPick={(t: Tag) => run({ action: 'remove_tags', tag_ids: [t.id] }, false)} />
      {:else if panel === 'category'}
        <strong>{b.category}</strong>
        <div class="row">
          <select class="select" bind:value={categoryId}>
            <option value="">{b.chooseCategory}</option>
            {#each catalog.tree as c (c.id)}
              <option value={String(c.id)}>{'  '.repeat(c.depth)}{c.name}</option>
            {/each}
          </select>
          <button class="btn sm" disabled={!categoryId || busy} onclick={() => run({ action: 'add_to_category', category_id: cat() })}>
            <IconCopy size={15} />{b.copyTo}
          </button>
          <button
            class="btn sm"
            disabled={!categoryId || busy}
            onclick={() => run({ action: 'move_to_category', category_id: cat(), from_category_id: filterCategory })}
          >
            <IconArrowsMove size={15} />{b.moveTo}
          </button>
          {#if filterCategory}
            <button class="btn sm" disabled={busy} onclick={() => run({ action: 'remove_from_category', category_id: filterCategory! })}>
              {b.removeFromCategory}
            </button>
          {/if}
        </div>
        <small class="muted">{b.moveHint}</small>
      {:else if panel === 'visibility'}
        <strong>{tr.visibility.title}</strong>
        <VisibilityPicker bind:mode={visMode} bind:profileIds={visProfiles} />
        <div class="row end">
          <button
            class="btn sm primary"
            disabled={busy || (visMode === 'profiles' && visProfiles.length === 0)}
            onclick={() => run({ action: 'set_visibility', mode: visMode, profile_ids: visProfiles })}
          >
            {tr.visibility.apply}
          </button>
        </div>
      {:else if panel === 'date'}
        <strong>{b.setDate}</strong>
        <div class="row">
          <input class="input" type="datetime-local" bind:value={date} />
          <button class="btn sm primary" disabled={!date || busy} onclick={() => run({ action: 'set_taken_at', taken_at: date })}>
            {tr.visibility.apply}
          </button>
        </div>
      {/if}
      {#if error}<div class="alert error">{error}</div>{/if}
    </div>
  {/if}

  <div class="bar">
    <strong class="count">{b.selected(selection.size)}</strong>
    <button class="btn sm ghost" onclick={onSelectAll}><IconSelectAll size={15} />{b.selectAll}</button>
    <button class="btn sm ghost" onclick={() => selection.clear()}><IconDeselect size={15} />{b.clear}</button>
    <span class="sep"></span>
    <button class="btn sm" class:on={panel === 'tag-add'} onclick={() => toggle('tag-add')}><IconTag size={15} />{b.addTag}</button>
    <button class="btn sm" class:on={panel === 'tag-remove'} onclick={() => toggle('tag-remove')}><IconTagOff size={15} />{b.removeTag}</button>
    <button class="btn sm" class:on={panel === 'category'} onclick={() => toggle('category')}><IconFolder size={15} />{b.category}</button>
    <button class="btn sm" class:on={panel === 'visibility'} onclick={() => toggle('visibility')}><IconShare size={15} />{tr.visibility.title}</button>
    <button class="btn sm" class:on={panel === 'date'} onclick={() => toggle('date')}><IconCalendar size={15} />{b.date}</button>
    <button class="btn sm" disabled={busy} onclick={() => run({ action: 'set_hidden', hidden: true })}><IconEyeOff size={15} />{b.hide}</button>
    <button class="btn sm" disabled={busy} onclick={() => run({ action: 'set_hidden', hidden: false })}><IconEye size={15} />{b.unhide}</button>
  </div>
</div>

<style>
  .bar-wrap {
    position: relative;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 8px 16px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    box-shadow: 0 -2px 10px rgba(30, 40, 60, 0.06);
  }
  .count {
    margin-right: 4px;
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--border);
    margin: 0 4px;
  }
  .btn.on {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .panel {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 16px;
    width: min(560px, calc(100% - 32px));
    padding: 12px 14px;
    display: grid;
    gap: 10px;
    z-index: 30;
  }
  .close {
    position: absolute;
    top: 6px;
    right: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .row .select {
    flex: 1;
    min-width: 200px;
  }
  .end {
    justify-content: flex-end;
  }
  small {
    font-size: 12px;
  }
</style>
