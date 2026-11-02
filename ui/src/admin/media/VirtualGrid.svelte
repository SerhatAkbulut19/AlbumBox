<script lang="ts">
  // Virtualized thumbnail grid: only visible rows are rendered, so archives
  // with 100k+ items stay smooth. Selection: click, Ctrl/⌘ toggle, Shift range,
  // drag a rectangle (marquee), checkbox on each cell. Double click opens.
  import { untrack } from 'svelte';
  import type { SvelteSet } from 'svelte/reactivity';
  import { thumbUrl } from '../../lib/api';
  import { tr } from '../../lib/i18n/tr';
  import { IconCheck, IconEyeOff, IconLock, IconPlayerPlay, IconWorld } from '../../lib/icons';
  import type { MediaSource } from './source.svelte';

  let {
    source,
    size,
    selection,
    onOpen,
  }: { source: MediaSource; size: number; selection: SvelteSet<number>; onOpen: (index: number) => void } = $props();

  const GAP = 6;
  const PAD = 12;

  let scroller: HTMLDivElement | undefined = $state();
  let width = $state(0);
  let height = $state(0);
  let scrollTop = $state(0);

  const cols = $derived(Math.max(1, Math.floor((width - PAD * 2 + GAP) / (size + GAP))));
  const cell = $derived(Math.max(40, (width - PAD * 2 - (cols - 1) * GAP) / cols));
  const rowH = $derived(cell + GAP);
  const rows = $derived(Math.ceil(source.total / cols));
  const firstRow = $derived(Math.max(0, Math.floor((scrollTop - PAD) / rowH) - 2));
  const lastRow = $derived(Math.min(rows - 1, Math.ceil((scrollTop + height) / rowH) + 2));
  const firstIndex = $derived(firstRow * cols);
  const lastIndex = $derived(Math.min(source.total - 1, (lastRow + 1) * cols - 1));
  const thumbSize = $derived<256 | 1024>(cell > 300 ? 1024 : 256);

  const visible = $derived.by(() => {
    void source.version;
    const out: number[] = [];
    for (let i = firstIndex; i <= lastIndex; i++) out.push(i);
    return out;
  });

  $effect(() => {
    const [a, b] = [firstIndex, lastIndex];
    if (b >= a) untrack(() => source.ensure(a, b));
  });

  // Back to top when the filter changes.
  $effect(() => {
    void source.loaded;
    if (!source.loaded && scroller) scroller.scrollTop = 0;
  });

  const pos = (i: number) => ({ top: PAD + Math.floor(i / cols) * rowH, left: PAD + (i % cols) * (cell + GAP) });

  // ---- selection -----------------------------------------------------------
  let anchor = -1;
  let drag: { x: number; y: number; base: Set<number>; moved: boolean; index: number; check: boolean; e: PointerEvent } | null =
    null;
  let marquee = $state<{ x: number; y: number; w: number; h: number } | null>(null);

  function point(e: PointerEvent) {
    const r = scroller!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top + scroller!.scrollTop };
  }

  function indexAt(x: number, y: number): number {
    const c = Math.floor((x - PAD) / (cell + GAP));
    const r = Math.floor((y - PAD) / rowH);
    if (c < 0 || c >= cols || r < 0) return -1;
    const inCell = x - PAD - c * (cell + GAP) <= cell && y - PAD - r * rowH <= cell;
    const i = r * cols + c;
    return inCell && i < source.total ? i : -1;
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || !scroller) return;
    const p = point(e);
    const check = !!(e.target as HTMLElement).closest('.check');
    drag = {
      ...p,
      base: e.ctrlKey || e.metaKey || e.shiftKey ? new Set(selection) : new Set(),
      moved: false,
      index: indexAt(p.x, p.y),
      check,
      e,
    };
    scroller.setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag || !scroller) return;
    const p = point(e);
    if (!drag.moved && Math.hypot(p.x - drag.x, p.y - drag.y) < 6) return;
    drag.moved = true;
    const x0 = Math.min(drag.x, p.x);
    const y0 = Math.min(drag.y, p.y);
    marquee = { x: x0, y: y0, w: Math.abs(p.x - drag.x), h: Math.abs(p.y - drag.y) };

    const c0 = Math.max(0, Math.floor((x0 - PAD) / (cell + GAP)));
    const c1 = Math.min(cols - 1, Math.floor((x0 + marquee.w - PAD) / (cell + GAP)));
    const r0 = Math.max(0, Math.floor((y0 - PAD) / rowH));
    const r1 = Math.floor((y0 + marquee.h - PAD) / rowH);
    const next = new Set(drag.base);
    for (let r = r0; r <= r1; r++) {
      for (let c = c0; c <= c1; c++) {
        const m = source.get(r * cols + c);
        if (m) next.add(m.id);
      }
    }
    selection.clear();
    for (const id of next) selection.add(id);

    // Auto-scroll near the edges.
    const r = scroller.getBoundingClientRect();
    if (e.clientY > r.bottom - 40) scroller.scrollTop += 24;
    else if (e.clientY < r.top + 40) scroller.scrollTop -= 24;
  }

  async function onPointerUp(e: PointerEvent) {
    if (!drag) return;
    const d = drag;
    drag = null;
    marquee = null;
    scroller?.releasePointerCapture(e.pointerId);
    if (d.moved) return;
    const i = d.index;
    const m = i >= 0 ? source.get(i) : undefined;
    if (!m) {
      if (!(d.e.ctrlKey || d.e.metaKey)) selection.clear();
      return;
    }
    if (d.e.shiftKey && anchor >= 0) {
      const ids = await source.idsBetween(anchor, i);
      if (!(d.e.ctrlKey || d.e.metaKey)) selection.clear();
      for (const id of ids) selection.add(id);
      return;
    }
    if (d.e.ctrlKey || d.e.metaKey || d.check) {
      if (selection.has(m.id)) selection.delete(m.id);
      else selection.add(m.id);
    } else {
      selection.clear();
      selection.add(m.id);
    }
    anchor = i;
  }

  function onDblClick(e: MouseEvent) {
    if (!scroller) return;
    const r = scroller.getBoundingClientRect();
    const i = indexAt(e.clientX - r.left, e.clientY - r.top + scroller.scrollTop);
    if (i >= 0) onOpen(i);
  }

  const duration = (ms: number | null) => {
    if (!ms) return '';
    const s = Math.round(ms / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
  };
</script>

<div
  class="scroller"
  bind:this={scroller}
  bind:clientWidth={width}
  bind:clientHeight={height}
  onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  ondblclick={onDblClick}
  role="grid"
  tabindex="-1"
>
  <div class="space" style:height={`${rows * rowH + PAD * 2}px`}>
    {#each visible as i (i)}
      {@const m = source.get(i)}
      {@const p = pos(i)}
      <div
        class="cell"
        class:selected={m && selection.has(m.id)}
        style:top={`${p.top}px`}
        style:left={`${p.left}px`}
        style:width={`${cell}px`}
        style:height={`${cell}px`}
      >
        {#if m}
          {#if m.thumb_ready === 1}
            <img src={thumbUrl(m.id, thumbSize)} alt={m.name ?? ''} draggable="false" loading="lazy" />
          {:else}
            <div class="noimg muted">{m.name}</div>
          {/if}
          <span class="check" aria-hidden="true"><IconCheck size={14} stroke={3} /></span>
          <span class="flags">
            {#if m.is_hidden}<span class="flag" title={tr.visibility.hidden}><IconEyeOff size={13} /></span>
            {:else if m.published}<span class="flag pub" title={tr.visibility.published}><IconWorld size={13} /></span>
            {:else}<span class="flag" title={tr.visibility.private}><IconLock size={13} /></span>{/if}
          </span>
          {#if m.kind === 'video'}
            <span class="video"><IconPlayerPlay size={12} />{duration(m.duration_ms)}</span>
          {/if}
        {/if}
      </div>
    {/each}
    {#if marquee}
      <div class="marquee" style:left={`${marquee.x}px`} style:top={`${marquee.y}px`} style:width={`${marquee.w}px`} style:height={`${marquee.h}px`}></div>
    {/if}
  </div>
</div>

<style>
  .scroller {
    position: relative;
    overflow-y: auto;
    overflow-x: hidden;
    height: 100%;
    user-select: none;
    outline: none;
  }
  .space {
    position: relative;
  }
  .cell {
    position: absolute;
    border-radius: 6px;
    overflow: hidden;
    background: var(--surface-2);
    cursor: default;
  }
  .cell img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    transition: transform 0.12s;
  }
  .cell.selected {
    outline: 3px solid var(--accent);
    outline-offset: -3px;
  }
  .cell.selected img {
    transform: scale(0.9);
    border-radius: 4px;
  }
  .noimg {
    display: grid;
    place-items: center;
    height: 100%;
    padding: 6px;
    font-size: 11px;
    text-align: center;
    word-break: break-all;
  }
  .check {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid rgba(255, 255, 255, 0.9);
    background: rgba(0, 0, 0, 0.25);
    color: transparent;
    display: grid;
    place-items: center;
    opacity: 0;
    transition: opacity 0.1s;
    cursor: pointer;
  }
  .cell:hover .check {
    opacity: 1;
  }
  .cell.selected .check {
    opacity: 1;
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .flags {
    position: absolute;
    top: 6px;
    right: 6px;
  }
  .flag {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.45);
    color: #fff;
  }
  .flag.pub {
    background: rgba(70, 140, 100, 0.85);
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
  .marquee {
    position: absolute;
    border: 1px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    pointer-events: none;
    z-index: 5;
  }
</style>
