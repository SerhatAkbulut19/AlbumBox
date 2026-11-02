<script lang="ts">
  import { api, type Tag } from '../../lib/api';
  import { catalog } from '../../lib/catalog.svelte';
  import { tr } from '../../lib/i18n/tr';
  import { IconFilterOff, IconSearch, IconX } from '../../lib/icons';
  import TagInput from '../../lib/TagInput.svelte';
  import { emptyFilter, type Filter } from './source.svelte';

  let {
    filter = $bindable(),
    size = $bindable(),
    total,
  }: { filter: Filter; size: number; total: number } = $props();

  const m = tr.media;
  let q = $state(filter.q);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let tagNames = $state<Record<number, Tag>>({});

  // Names for tag ids that came from the URL.
  $effect(() => {
    const missing = filter.tags.filter((id) => !tagNames[id]);
    if (missing.length) {
      api.get<Tag[]>('/admin/tags').then((all) => {
        for (const t of all) if (filter.tags.includes(t.id)) tagNames[t.id] = t;
      });
    }
  });

  function onSearch() {
    clearTimeout(timer);
    timer = setTimeout(() => (filter = { ...filter, q }), 300);
  }

  function set<K extends keyof Filter>(key: K, value: Filter[K]) {
    filter = { ...filter, [key]: value };
  }

  function addTag(t: Tag) {
    tagNames[t.id] = t;
    if (!filter.tags.includes(t.id)) set('tags', [...filter.tags, t.id]);
  }

  function clearAll() {
    q = '';
    filter = emptyFilter();
  }

  const years = Array.from({ length: new Date().getFullYear() - 1969 }, (_, i) => new Date().getFullYear() - i);
  const active = $derived(
    !!(filter.q || filter.category || filter.tags.length || filter.untagged || filter.kind || filter.year || filter.visibility || filter.profile || filter.library),
  );
</script>

<div class="filters">
  <div class="row">
    <label class="search">
      <IconSearch size={16} class="search-icon" />
      <input class="input" bind:value={q} oninput={onSearch} placeholder={m.search} />
    </label>
    <select class="select" value={filter.category} onchange={(e) => set('category', e.currentTarget.value)}>
      <option value="">{m.allCategories}</option>
      <option value="none">{m.uncategorized}</option>
      {#each catalog.tree as c (c.id)}
        <option value={String(c.id)}>{'  '.repeat(c.depth)}{c.name} ({c.media_count})</option>
      {/each}
    </select>
    <select class="select narrow" value={filter.kind} onchange={(e) => set('kind', e.currentTarget.value)}>
      <option value="">{m.allKinds}</option>
      <option value="photo">{m.photos}</option>
      <option value="video">{m.videos}</option>
    </select>
    <select class="select narrow" value={filter.year} onchange={(e) => set('year', e.currentTarget.value)}>
      <option value="">{m.year}</option>
      {#each years as y}<option value={String(y)}>{y}</option>{/each}
    </select>
    <select class="select" value={filter.visibility} onchange={(e) => set('visibility', e.currentTarget.value)}>
      {#each Object.entries(m.visibility) as [value, label]}<option {value}>{label}</option>{/each}
    </select>
    <select class="select" value={filter.profile} onchange={(e) => set('profile', e.currentTarget.value)}>
      <option value="">{m.allProfiles}</option>
      {#each catalog.profiles as p (p.id)}<option value={String(p.id)}>{p.name}</option>{/each}
    </select>
    <select class="select narrow" value={filter.sort} onchange={(e) => set('sort', e.currentTarget.value)}>
      {#each Object.entries(m.sort) as [value, label]}<option value={value === 'taken_desc' ? '' : value}>{label}</option>{/each}
    </select>
  </div>
  <div class="row">
    <div class="tagbox"><TagInput onPick={addTag} placeholder={m.tagFilter} /></div>
    {#each filter.tags as id (id)}
      <span class="chip">
        <span class="dot" style:background={catalog.typeById(tagNames[id]?.type_id ?? 0)?.color ?? 'var(--accent)'}></span>
        {tagNames[id]?.name ?? `#${id}`}
        <button class="x" onclick={() => set('tags', filter.tags.filter((t) => t !== id))} aria-label={tr.common.close}><IconX size={12} /></button>
      </span>
    {/each}
    <label class="check small"><input type="checkbox" checked={filter.untagged} onchange={(e) => set('untagged', e.currentTarget.checked)} />{m.untagged}</label>
    {#if filter.library}
      <span class="chip">
        {filter.folder || '…'}
        <button class="x" onclick={() => (filter = { ...filter, library: '', folder: '' })} aria-label={tr.common.close}><IconX size={12} /></button>
      </span>
    {/if}
    {#if active}
      <button class="btn sm ghost" onclick={clearAll}><IconFilterOff size={15} />{m.clearFilters}</button>
    {/if}
    <span class="spacer"></span>
    <span class="muted small">{m.count(total)}</span>
    <label class="zoom muted small">
      {m.zoom}
      <input type="range" min="90" max="320" step="10" bind:value={size} />
    </label>
  </div>
</div>

<style>
  .filters {
    padding: 12px 16px 10px;
    display: grid;
    gap: 8px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .row .select {
    width: auto;
    max-width: 230px;
  }
  .narrow {
    max-width: 150px;
  }
  .search {
    position: relative;
    flex: 1;
    min-width: 220px;
  }
  .search :global(.search-icon) {
    position: absolute;
    left: 9px;
    top: 8px;
    color: var(--text-muted);
  }
  .search .input {
    padding-left: 32px;
  }
  .tagbox {
    width: 260px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 4px 0 10px;
    border-radius: 999px;
    background: var(--accent-soft);
    font-size: 13px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .x {
    border: none;
    background: transparent;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    cursor: pointer;
    color: var(--text-muted);
  }
  .x:hover {
    background: var(--surface);
  }
  .spacer {
    flex: 1;
  }
  .small {
    font-size: 12px;
  }
  .zoom {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .zoom input {
    width: 110px;
    accent-color: var(--accent);
  }
</style>
