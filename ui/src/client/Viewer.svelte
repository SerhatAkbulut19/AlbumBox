<script lang="ts">
  // Family viewer: photo or video, ←/→, info panel with tags and albums.
  // Tags lead to a search, albums open the album.
  import { untrack } from 'svelte';
  import { api, originalUrl, thumbUrl, type ClientItem, type MediaInfo } from '../lib/api';
  import { tr, longDate } from '../lib/i18n/tr';
  import { IconChevronLeft, IconChevronRight, IconDownload, IconInfoCircle, IconX } from '../lib/icons';
  import { navigate } from '../lib/router.svelte';

  let {
    items,
    total,
    index = $bindable(),
    onClose,
    onMore,
  }: { items: ClientItem[]; total: number; index: number; onClose: () => void; onMore: () => void } = $props();

  let info = $state<MediaInfo | null>(null);
  let showInfo = $state(localStorage.getItem('albumbox.viewerInfo') !== '0');
  const item = $derived(items[index]);

  $effect(() => {
    const id = item?.id;
    const i = index;
    untrack(() => {
      info = null;
      if (id) api.get<MediaInfo>(`/media/${id}`).then((r) => item?.id === id && (info = r)).catch(() => {});
      if (i >= items.length - 3) onMore();
    });
  });

  $effect(() => localStorage.setItem('albumbox.viewerInfo', showInfo ? '1' : '0'));

  function go(delta: number) {
    const next = index + delta;
    if (next >= 0 && next < items.length) index = next;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowRight') go(1);
    else if (e.key === 'ArrowLeft') go(-1);
    else if (e.key === 'Escape') onClose();
    else if (e.key.toLowerCase() === 'i') showInfo = !showInfo;
  }

  function goto(url: string) {
    onClose();
    navigate(url);
  }

  // Swipe on touch screens.
  let touchX: number | null = null;
</script>

<svelte:window onkeydown={onKey} />

<div class="viewer" class:with-info={showInfo}>
  <!-- Swipe is a touch shortcut; keyboard users have ←/→ and the buttons. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="stage"
    ontouchstart={(e) => (touchX = e.touches[0].clientX)}
    ontouchend={(e) => {
      if (touchX === null) return;
      const dx = e.changedTouches[0].clientX - touchX;
      if (Math.abs(dx) > 50) go(dx < 0 ? 1 : -1);
      touchX = null;
    }}
  >
    {#if item}
      {#key item.id}
        {#if item.kind === 'video'}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video src={originalUrl(item.id)} poster={thumbUrl(item.id, 1024)} controls autoplay preload="metadata"></video>
        {:else}
          <img src={thumbUrl(item.id, 1024)} alt="" />
        {/if}
      {/key}
    {/if}
    <button class="nav prev" onclick={() => go(-1)} disabled={index <= 0} aria-label={tr.viewer.prev}><IconChevronLeft size={28} /></button>
    <button class="nav next" onclick={() => go(1)} disabled={index >= items.length - 1} aria-label={tr.viewer.next}><IconChevronRight size={28} /></button>
    <div class="top">
      <span class="pos">{index + 1} / {total}</span>
      <span class="spacer"></span>
      {#if item}
        <a class="icon-btn" href={originalUrl(item.id)} download title={tr.photo.download}><IconDownload size={20} /></a>
      {/if}
      <button class="icon-btn" class:on={showInfo} onclick={() => (showInfo = !showInfo)} title={tr.photo.info}><IconInfoCircle size={20} /></button>
      <button class="icon-btn" onclick={onClose} title={tr.common.close}><IconX size={22} /></button>
    </div>
  </div>

  {#if showInfo}
    <aside class="info">
      {#if info}
        <div class="date">{longDate(info.taken_at) || tr.media.noDate}</div>
        {#if info.camera || info.width}
          <div class="muted small">{#if info.camera}{info.camera} · {/if}{#if info.width}{info.width}×{info.height}{/if}</div>
        {/if}
        {#if info.tags.length}
          <h3>{tr.photo.tags}</h3>
          <div class="chips">
            {#each info.tags as t (t.id)}
              <button class="chip" onclick={() => goto(`/search?tags=${t.id}`)}>
                <span class="dot" style:background={t.color ?? 'var(--accent)'}></span>{t.name}
              </button>
            {/each}
          </div>
        {/if}
        {#if info.albums.length}
          <h3>{tr.photo.albums}</h3>
          <div class="chips">
            {#each info.albums as [id, name] (id)}
              <button class="chip" onclick={() => goto(`/album/${id}`)}>{name}</button>
            {/each}
          </div>
        {/if}
      {/if}
    </aside>
  {/if}
</div>

<style>
  .viewer {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    grid-template-columns: 1fr;
    background: #0f1115;
    color: #fff;
  }
  .viewer.with-info {
    grid-template-columns: 1fr 300px;
  }
  .stage {
    position: relative;
    display: grid;
    place-items: center;
    min-width: 0;
    min-height: 0;
    padding: 56px 64px 24px;
  }
  .stage img,
  .stage video {
    max-width: 100%;
    max-height: calc(100vh - 80px);
    object-fit: contain;
    border-radius: 4px;
  }
  .nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 44px;
    height: 44px;
    border-radius: 50%;
    border: none;
    background: rgba(255, 255, 255, 0.08);
    color: #fff;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .nav:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.18);
  }
  .nav:disabled {
    opacity: 0.2;
  }
  .prev {
    left: 12px;
  }
  .next {
    right: 12px;
  }
  .top {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 10px 12px;
  }
  .pos {
    color: rgba(255, 255, 255, 0.65);
    font-size: 13px;
  }
  .spacer {
    flex: 1;
  }
  .icon-btn {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border: none;
    background: transparent;
    color: #fff;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .icon-btn:hover,
  .icon-btn.on {
    background: rgba(255, 255, 255, 0.12);
  }
  .info {
    background: #171a21;
    border-left: 1px solid #262b36;
    padding: 56px 16px 16px;
    display: grid;
    align-content: start;
    gap: 8px;
    overflow-y: auto;
  }
  .date {
    font-size: 15px;
    font-weight: 600;
  }
  .muted {
    color: #8f98aa;
  }
  .small {
    font-size: 12px;
  }
  h3 {
    margin-top: 10px;
    font-size: 11px;
    font-weight: 600;
    color: #8f98aa;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border-radius: 999px;
    border: 1px solid #323846;
    background: #1f232c;
    color: #dfe3eb;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .chip:hover {
    border-color: #7f9fd1;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  @media (max-width: 760px) {
    .viewer.with-info {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr auto;
    }
    .info {
      border-left: none;
      border-top: 1px solid #262b36;
      padding-top: 12px;
      max-height: 35vh;
    }
    .stage {
      padding: 56px 8px 8px;
    }
  }
</style>
