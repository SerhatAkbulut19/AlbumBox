<script lang="ts">
  // Cover card used for albums and search groups.
  import { thumbUrl } from '../lib/api';
  import { dateRange } from '../lib/i18n/tr';
  import { IconPhoto } from '../lib/icons';

  let {
    title,
    count,
    cover,
    from = null,
    to = null,
    unit,
    href = null,
    onclick = undefined,
  }: {
    title: string;
    count: number;
    cover: number | null;
    from?: string | null;
    to?: string | null;
    unit: string;
    href?: string | null;
    onclick?: () => void;
  } = $props();
</script>

<svelte:element
  this={href ? 'a' : 'button'}
  class="card album"
  {href}
  role={href ? undefined : 'button'}
  onclick={(e: MouseEvent) => {
    if (onclick) {
      e.preventDefault();
      onclick();
    }
  }}
>
  <div class="cover">
    {#if cover}
      <img src={thumbUrl(cover, 1024)} alt="" loading="lazy" />
    {:else}
      <div class="empty"><IconPhoto size={32} stroke={1.25} /></div>
    {/if}
  </div>
  <div class="meta">
    <strong title={title}>{title}</strong>
    <span class="muted">{count.toLocaleString('tr-TR')} {unit}{#if from}{' · '}{dateRange(from, to)}{/if}</span>
  </div>
</svelte:element>

<style>
  .album {
    display: grid;
    grid-template-rows: auto auto;
    overflow: hidden;
    padding: 0;
    text-align: left;
    color: inherit;
    cursor: pointer;
    font: inherit;
    transition:
      transform 0.15s,
      box-shadow 0.15s;
  }
  .album:hover {
    transform: translateY(-2px);
    box-shadow: 0 6px 18px rgba(30, 40, 60, 0.12);
  }
  .cover {
    aspect-ratio: 4 / 3;
    background: var(--surface-2);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .empty {
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }
  .meta {
    padding: 10px 12px;
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .meta strong {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta span {
    font-size: 12px;
  }
</style>
