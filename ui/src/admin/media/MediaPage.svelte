<script lang="ts">
  import { untrack } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { catalog } from '../../lib/catalog.svelte';
  import { engine } from '../../lib/engine.svelte';
  import { tr } from '../../lib/i18n/tr';
  import { replaceQuery, router } from '../../lib/router.svelte';
  import BulkBar from './BulkBar.svelte';
  import MediaFilters from './MediaFilters.svelte';
  import MediaViewer from './MediaViewer.svelte';
  import { MediaSource, filterToParams, paramsToFilter, type Filter } from './source.svelte';
  import VirtualGrid from './VirtualGrid.svelte';

  catalog.ensure();

  const source = new MediaSource();
  const selection = new SvelteSet<number>();
  let filter = $state<Filter>(paramsToFilter(new URLSearchParams(router.search)));
  let size = $state(Number(localStorage.getItem('albumbox.gridSize')) || 150);
  let viewerIndex = $state<number | null>(null);
  let notice = $state('');
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  // Links from other pages (?category=3, ?profile=2 …) update the filter.
  let lastSearch = router.search;
  $effect(() => {
    if (router.search !== lastSearch && router.search !== `?${filterToParams(filter)}`) {
      lastSearch = router.search;
      filter = paramsToFilter(new URLSearchParams(router.search));
    }
  });

  // Filter → URL + data. Selection is kept: it holds ids, not positions.
  $effect(() => {
    const params = filterToParams(filter);
    untrack(() => {
      replaceQuery(params);
      lastSearch = location.search;
      source.setFilter(params);
    });
  });

  $effect(() => localStorage.setItem('albumbox.gridSize', String(size)));

  // New scan results show up without a manual refresh.
  let seenVersion = engine.snap?.libraries_version ?? 0;
  $effect(() => {
    const v = engine.snap?.libraries_version ?? 0;
    if (v !== seenVersion) {
      seenVersion = v;
      untrack(() => source.refresh());
    }
  });

  async function selectAll() {
    for (const id of await source.allIds()) selection.add(id);
  }

  function done(message: string) {
    notice = message;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ''), 3000);
    source.refresh();
  }

  function onKey(e: KeyboardEvent) {
    if (viewerIndex !== null || (e.target as HTMLElement).closest('input, select, textarea')) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      selectAll();
    } else if (e.key === 'Escape') {
      selection.clear();
    }
  }

  const filterCategory = $derived(filter.category && filter.category !== 'none' ? Number(filter.category) : null);
</script>

<svelte:window onkeydown={onKey} />

<div class="media-page">
  <MediaFilters bind:filter bind:size total={source.total} />
  <div class="grid-area">
    {#if source.loaded && source.total === 0}
      <div class="empty muted">{tr.media.empty}</div>
    {:else}
      <VirtualGrid {source} {size} {selection} onOpen={(i) => (viewerIndex = i)} />
    {/if}
    {#if notice}<div class="toast alert success">{notice}</div>{/if}
  </div>
  {#if selection.size > 0}
    <BulkBar {selection} {filterCategory} onSelectAll={selectAll} onDone={done} />
  {:else}
    <div class="hint muted">{tr.media.selectHint}</div>
  {/if}
</div>

{#if viewerIndex !== null}
  <MediaViewer {source} bind:index={viewerIndex} onClose={() => (viewerIndex = null)} onChanged={() => source.refresh()} />
{/if}

<style>
  .media-page {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: 100%;
    min-height: 0;
  }
  .grid-area {
    position: relative;
    min-height: 0;
  }
  .empty {
    padding: 48px;
    text-align: center;
  }
  .hint {
    padding: 7px 16px;
    font-size: 12px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
  .toast {
    position: absolute;
    right: 16px;
    bottom: 12px;
    box-shadow: var(--shadow);
  }
</style>
