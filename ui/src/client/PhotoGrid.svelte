<script lang="ts">
  // Square thumbnail grid with infinite loading. Clicking opens the viewer
  // over the loaded items; the viewer can ask for more via `more()`.
  import { thumbUrl, type ClientItem, type ClientPage } from '../lib/api';
  import { tr } from '../lib/i18n/tr';
  import { IconPlayerPlay } from '../lib/icons';
  import Viewer from './Viewer.svelte';

  let { load, emptyText = '' }: { load: (offset: number) => Promise<ClientPage>; emptyText?: string } = $props();

  let items = $state<ClientItem[]>([]);
  let total = $state(0);
  let loaded = $state(false);
  let loading = false;
  let open = $state<number | null>(null);
  let sentinel: HTMLDivElement | undefined = $state();

  export async function more() {
    if (loading || (loaded && items.length >= total)) return;
    loading = true;
    try {
      const page = await load(items.length);
      items = [...items, ...page.items];
      total = page.total;
      loaded = true;
    } finally {
      loading = false;
    }
  }

  more();

  $effect(() => {
    if (!sentinel) return;
    const io = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && more(), { rootMargin: '600px' });
    io.observe(sentinel);
    return () => io.disconnect();
  });

  const duration = (ms: number | null) => {
    if (!ms) return '';
    const s = Math.round(ms / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
  };
</script>

{#if loaded && items.length === 0}
  <div class="card empty muted">{emptyText || tr.search.none}</div>
{:else}
  <div class="grid">
    {#each items as m, i (m.id)}
      <button class="cell" onclick={() => (open = i)}>
        {#if m.thumb_ready === 1}<img src={thumbUrl(m.id)} alt="" loading="lazy" />{/if}
        {#if m.kind === 'video'}<span class="video"><IconPlayerPlay size={12} />{duration(m.duration_ms)}</span>{/if}
      </button>
    {/each}
  </div>
  <div bind:this={sentinel} class="sentinel"></div>
{/if}

{#if open !== null}
  <Viewer {items} {total} bind:index={open} onClose={() => (open = null)} onMore={more} />
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 4px;
  }
  .cell {
    position: relative;
    aspect-ratio: 1;
    padding: 0;
    border: none;
    border-radius: 4px;
    overflow: hidden;
    background: var(--surface-2);
    cursor: zoom-in;
  }
  .cell img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    transition: transform 0.2s;
  }
  .cell:hover img {
    transform: scale(1.04);
  }
  .video {
    position: absolute;
    bottom: 6px;
    left: 6px;
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 1px 6px 1px 4px;
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    font-size: 11px;
  }
  .sentinel {
    height: 1px;
  }
  .empty {
    padding: 28px;
    text-align: center;
  }
</style>
