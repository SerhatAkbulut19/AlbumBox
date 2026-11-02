<script lang="ts">
  // Autocomplete for tags. Picks an existing tag or (with allowCreate) creates
  // one in the chosen type. Turkish-insensitive: "koy" finds "Köy".
  import { api, type Tag } from './api';
  import { catalog } from './catalog.svelte';
  import { tr } from './i18n/tr';

  let {
    onPick,
    allowCreate = false,
    placeholder = tr.tagInput.placeholder,
    typeId = null,
  }: {
    onPick: (tag: Tag) => void;
    allowCreate?: boolean;
    placeholder?: string;
    /** Restrict suggestions (and creation) to one type. */
    typeId?: number | null;
  } = $props();

  let text = $state('');
  let results = $state<Tag[]>([]);
  let open = $state(false);
  let active = $state(0);
  // Type for new tags when `typeId` is not fixed (default: Etkinlik).
  let createType = $state<number>(3);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const trimmed = $derived(text.trim());
  const exact = $derived(results.some((r) => r.name.toLocaleLowerCase('tr') === trimmed.toLocaleLowerCase('tr')));
  const showCreate = $derived(allowCreate && trimmed.length > 0 && !exact);
  const optionCount = $derived(results.length + (showCreate ? 1 : 0));

  function search() {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      if (!trimmed) {
        results = [];
        return;
      }
      const type = typeId ? `&type=${typeId}` : '';
      results = await api.get<Tag[]>(`/admin/tags?q=${encodeURIComponent(trimmed)}${type}`).catch(() => []);
      active = 0;
      open = true;
    }, 150);
  }

  async function choose(i: number) {
    let tag: Tag | undefined = results[i];
    if (!tag && showCreate) {
      tag = await api.post<Tag>('/admin/tags', { type_id: typeId ?? createType, name: trimmed });
      catalog.reloadTagTypes();
    }
    if (!tag) return;
    onPick(tag);
    text = '';
    results = [];
    open = false;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      active = Math.min(optionCount - 1, active + 1);
      e.preventDefault();
    } else if (e.key === 'ArrowUp') {
      active = Math.max(0, active - 1);
      e.preventDefault();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (optionCount > 0) choose(active);
    } else if (e.key === 'Escape') {
      open = false;
    }
  }

  const color = (t: Tag) => catalog.typeById(t.type_id)?.color ?? 'var(--accent)';
  const typeName = (id: number) => catalog.typeById(id)?.name ?? '';
</script>

<div class="tag-input">
  <div class="row">
    <input
      class="input"
      bind:value={text}
      {placeholder}
      oninput={search}
      onkeydown={onKey}
      onfocus={() => (open = results.length > 0)}
      onblur={() => setTimeout(() => (open = false), 150)}
    />
    {#if allowCreate && !typeId}
      <select class="select type" bind:value={createType} title={tr.tags.newType}>
        {#each catalog.tagTypes as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
      </select>
    {/if}
  </div>
  {#if open && optionCount > 0}
    <div class="menu card">
      {#each results as r, i (r.id)}
        <button class="opt" class:active={i === active} onmousedown={(e) => (e.preventDefault(), choose(i))}>
          <span class="dot" style:background={color(r)}></span>
          <span class="name">{r.name}</span>
          {#if r.parent_name}<span class="muted small">{r.parent_name}</span>{/if}
          <span class="muted small type">{typeName(r.type_id)} · {r.media_count}</span>
        </button>
      {/each}
      {#if showCreate}
        <button class="opt create" class:active={active === results.length} onmousedown={(e) => (e.preventDefault(), choose(results.length))}>
          + {tr.tagInput.create(trimmed, typeName(typeId ?? createType))}
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tag-input {
    position: relative;
    min-width: 0;
  }
  .row .input {
    flex: 1;
    min-width: 0;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .type {
    width: 110px;
    flex: none;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 40;
    max-height: 280px;
    overflow: auto;
    padding: 4px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    background: transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
  }
  .opt.active,
  .opt:hover {
    background: var(--accent-soft);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }
  .name {
    flex: 1;
  }
  .small {
    font-size: 12px;
  }
  .type {
    white-space: nowrap;
  }
  .create {
    color: var(--accent);
  }
</style>
