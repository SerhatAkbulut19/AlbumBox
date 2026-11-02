<script lang="ts">
  // Full-screen viewer with an editable info panel (tags, categories,
  // publishing, date). ←/→ moves through the current list.
  import { untrack } from 'svelte';
  import { api, ApiError, bulk, originalUrl, thumbUrl, type BulkOp, type MediaDetail, type MediaFace, type Tag } from '../../lib/api';
  import { catalog } from '../../lib/catalog.svelte';
  import { tr, errorText } from '../../lib/i18n/tr';
  import {
    IconChevronLeft,
    IconChevronRight,
    IconExternalLink,
    IconFaceId,
    IconEye,
    IconEyeOff,
    IconX,
  } from '../../lib/icons';
  import TagInput from '../../lib/TagInput.svelte';
  import VisibilityPicker from '../../lib/VisibilityPicker.svelte';
  import type { MediaSource } from './source.svelte';

  let {
    source,
    index = $bindable(),
    onClose,
    onChanged,
  }: { source: MediaSource; index: number; onClose: () => void; onChanged: () => void } = $props();

  const v = tr.viewer;
  let detail = $state<MediaDetail | null>(null);
  let faces = $state<MediaFace[]>([]);
  let showFaces = $state(true);
  let error = $state('');
  let visMode = $state<'inherit' | 'private' | 'profiles'>('inherit');
  let visProfiles = $state<number[]>([]);
  let date = $state('');

  const item = $derived.by(() => {
    void source.version;
    return source.get(index);
  });

  async function load(id: number) {
    error = '';
    faces = [];
    try {
      const d = await api.get<MediaDetail>(`/admin/media/${id}`);
      if (source.get(index)?.id !== id) return;
      detail = d;
      faces = await api.get<MediaFace[]>(`/admin/media/${id}/faces`).catch(() => []);
      visMode = d.visibility_mode === 'inherit' ? 'inherit' : d.share_profile_ids.length ? 'profiles' : 'private';
      visProfiles = [...d.share_profile_ids];
      date = d.taken_at?.slice(0, 16) ?? '';
    } catch (e) {
      error = errorText(e instanceof ApiError ? e.code : 'internal');
    }
  }

  $effect(() => {
    const i = index;
    const id = item?.id;
    untrack(() => {
      source.ensure(Math.max(0, i - 1), i + 1);
      if (id) load(id);
    });
  });

  function go(delta: number) {
    const next = index + delta;
    if (next >= 0 && next < source.total) {
      detail = null;
      index = next;
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.target as HTMLElement).closest('input, select, textarea')) return;
    if (e.key === 'ArrowRight') go(1);
    else if (e.key === 'ArrowLeft') go(-1);
    else if (e.key === 'Escape') onClose();
  }

  async function apply(op: BulkOp) {
    if (!detail) return;
    try {
      await bulk([detail.id], op);
      await load(detail.id);
      onChanged();
      if (op.action.includes('categor')) catalog.reloadCategories();
    } catch (e) {
      error = errorText(e instanceof ApiError ? e.code : 'internal');
    }
  }

  const fmtDate = (s: string | null) =>
    s ? new Date(s).toLocaleString('tr-TR', { dateStyle: 'long', timeStyle: 'short' }) : tr.media.noDate;
  const fmtSize = (n: number) => (n > 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : `${Math.round(n / 1024)} KB`);
</script>

<svelte:window onkeydown={onKey} />

<div class="viewer">
  <div class="stage">
    <button class="nav prev" onclick={() => go(-1)} disabled={index <= 0} aria-label={v.prev}><IconChevronLeft size={28} /></button>
    {#if item}
      {#key item.id}
        {#if item.kind === 'video'}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video src={originalUrl(item.id)} poster={thumbUrl(item.id, 1024)} controls preload="metadata"></video>
        {:else}
          <div class="photo">
            <img src={thumbUrl(item.id, 1024)} alt={item.name ?? ''} />
            {#if showFaces}
              {#each faces as f (f.id)}
                <div
                  class="facebox {f.status}"
                  style:left={`${f.bbox[0] * 100}%`}
                  style:top={`${f.bbox[1] * 100}%`}
                  style:width={`${f.bbox[2] * 100}%`}
                  style:height={`${f.bbox[3] * 100}%`}
                >
                  <span>{f.person_name ? (f.status === 'suggested' ? `${f.person_name}?` : f.person_name) : tr.persons.unknown}</span>
                </div>
              {/each}
            {/if}
          </div>
        {/if}
      {/key}
    {/if}
    <button class="nav next" onclick={() => go(1)} disabled={index >= source.total - 1} aria-label={v.next}><IconChevronRight size={28} /></button>
    <span class="pos">{index + 1} / {source.total}</span>
    {#if faces.length}
      <button class="faces-toggle" class:on={showFaces} onclick={() => (showFaces = !showFaces)} title={tr.nav.persons}>
        <IconFaceId size={18} />{faces.length}
      </button>
    {/if}
  </div>

  <aside class="info">
    <header>
      <strong class="title" title={detail?.name ?? ''}>{item?.name ?? ''}</strong>
      <button class="btn sm ghost" onclick={onClose} aria-label={tr.common.close}><IconX size={18} /></button>
    </header>
    {#if error}<div class="alert error">{error}</div>{/if}
    {#if detail}
      <section>
        <h3>{v.date}</h3>
        <div>{fmtDate(detail.taken_at)}
          {#if detail.taken_at_source}<span class="muted small">· {v.source[detail.taken_at_source] ?? detail.taken_at_source}</span>{/if}
        </div>
        <div class="row">
          <input class="input" type="datetime-local" bind:value={date} />
          <button class="btn sm" disabled={!date || date === detail.taken_at?.slice(0, 16)} onclick={() => apply({ action: 'set_taken_at', taken_at: date })}>
            {tr.common.save}
          </button>
        </div>
        {#if detail.camera || detail.width}
          <div class="muted small">
            {#if detail.camera}{v.camera}: {detail.camera} · {/if}{#if detail.width}{detail.width}×{detail.height}{/if}
          </div>
        {/if}
      </section>

      <section>
        <h3>{v.tags}</h3>
        <div class="chips">
          {#each detail.tags as t (t.id)}
            <span class="chip" title={t.type_name}>
              <span class="dot" style:background={t.color ?? 'var(--accent)'}></span>{t.name}
              <button class="x" onclick={() => apply({ action: 'remove_tags', tag_ids: [t.id] })} aria-label={tr.bulk.removeTag}><IconX size={12} /></button>
            </span>
          {/each}
        </div>
        <TagInput allowCreate onPick={(t: Tag) => apply({ action: 'add_tags', tag_ids: [t.id] })} />
      </section>

      <section>
        <h3>{v.categories}</h3>
        <div class="chips">
          {#each detail.categories as [id, name] (id)}
            <span class="chip">{name}
              <button class="x" onclick={() => apply({ action: 'remove_from_category', category_id: id })} aria-label={tr.bulk.removeFromCategory}><IconX size={12} /></button>
            </span>
          {/each}
        </div>
        <select
          class="select"
          value=""
          onchange={(e) => {
            const id = Number(e.currentTarget.value);
            e.currentTarget.value = '';
            if (id) apply({ action: 'add_to_category', category_id: id });
          }}
        >
          <option value="">{v.addCategory}</option>
          {#each catalog.tree as c (c.id)}<option value={String(c.id)}>{'  '.repeat(c.depth)}{c.name}</option>{/each}
        </select>
      </section>

      <section>
        <h3>{tr.visibility.title}</h3>
        <VisibilityPicker bind:mode={visMode} bind:profileIds={visProfiles} />
        <div class="row end">
          <button
            class="btn sm primary"
            disabled={visMode === 'profiles' && visProfiles.length === 0}
            onclick={() => apply({ action: 'set_visibility', mode: visMode, profile_ids: visProfiles })}
          >{tr.visibility.apply}</button>
        </div>
        <div class="small">
          <span class="muted">{v.audience}:</span>
          {#if detail.is_hidden}<span class="badge danger">{tr.visibility.hidden}</span>
          {:else if detail.access_profile_ids.length === 0}<span class="badge">{v.nobody}</span>
          {:else}{#each detail.access_profile_ids as p}<span class="badge success">{catalog.profileName(p)}</span> {/each}{/if}
        </div>
        <div>
          {#if detail.is_hidden}
            <button class="btn sm" onclick={() => apply({ action: 'set_hidden', hidden: false })}><IconEye size={15} />{tr.bulk.unhide}</button>
          {:else}
            <button class="btn sm" onclick={() => apply({ action: 'set_hidden', hidden: true })}><IconEyeOff size={15} />{tr.bulk.hide}</button>
          {/if}
        </div>
      </section>

      <section>
        <h3>{v.files}</h3>
        {#each detail.files as f}
          <div class="file small">
            <span class="muted">{f.library_name}:</span> <span class="path">{f.rel_path}</span>
            <span class="muted">· {fmtSize(f.size)}</span>
            {#if f.is_missing}<span class="badge danger">{v.missing}</span>{:else if !f.library_online}<span class="badge">{v.offline}</span>{/if}
          </div>
        {/each}
        {#if item?.kind === 'photo'}
          <a class="btn sm" href={originalUrl(detail.id)} target="_blank" rel="noopener"><IconExternalLink size={15} />Orijinal</a>
        {/if}
      </section>
    {/if}
  </aside>
</div>

<style>
  .viewer {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    grid-template-columns: 1fr 340px;
    background: #0f1115;
  }
  .stage {
    position: relative;
    display: grid;
    place-items: center;
    min-width: 0;
    min-height: 0;
    padding: 24px 64px;
  }
  .stage img,
  .stage video {
    max-width: 100%;
    max-height: calc(100vh - 48px);
    object-fit: contain;
    border-radius: 4px;
  }
  .photo {
    position: relative;
    display: inline-block;
    line-height: 0;
  }
  .photo img {
    display: block;
  }
  .facebox {
    position: absolute;
    border: 2px solid rgba(255, 255, 255, 0.85);
    border-radius: 4px;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.35);
  }
  .facebox.suggested {
    border-style: dashed;
    border-color: #f0c674;
  }
  .facebox.unassigned {
    border-color: rgba(255, 255, 255, 0.45);
  }
  .facebox span {
    position: absolute;
    top: calc(100% + 3px);
    left: 50%;
    transform: translateX(-50%);
    white-space: nowrap;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.65);
    color: #fff;
    font-size: 12px;
    line-height: 1.5;
  }
  .faces-toggle {
    position: absolute;
    bottom: 8px;
    left: 12px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border: none;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    font: inherit;
    font-size: 12px;
  }
  .faces-toggle.on {
    background: rgba(255, 255, 255, 0.22);
    color: #fff;
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
    opacity: 0.25;
  }
  .prev {
    left: 12px;
  }
  .next {
    right: 12px;
  }
  .pos {
    position: absolute;
    bottom: 10px;
    left: 50%;
    transform: translateX(-50%);
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
  }
  .info {
    background: var(--surface);
    border-left: 1px solid var(--border);
    overflow-y: auto;
    overflow-x: hidden;
    min-width: 0;
    display: grid;
    align-content: start;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--surface);
    z-index: 1;
  }
  .title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  section {
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    display: grid;
    gap: 8px;
  }
  h3 {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .end {
    justify-content: flex-end;
  }
  .small {
    font-size: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chips:empty {
    display: none;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 2px 0 9px;
    border-radius: 999px;
    background: var(--accent-soft);
    font-size: 12px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .x {
    border: none;
    background: transparent;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    cursor: pointer;
    color: var(--text-muted);
  }
  .x:hover {
    background: var(--surface);
  }
  .file {
    word-break: break-all;
  }
  .badge {
    margin-right: 4px;
  }
</style>
