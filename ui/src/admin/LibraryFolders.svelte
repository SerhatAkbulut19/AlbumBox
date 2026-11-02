<script lang="ts">
  // Folders of one library with previews; folder names become event tags.
  import { api, ApiError, thumbUrl, type FolderSummary } from '../lib/api';
  import { tr, errorText } from '../lib/i18n/tr';
  import { IconFolder, IconTag } from '../lib/icons';

  let { libraryId, version }: { libraryId: string; version: number } = $props();

  const f = tr.folders;
  let folders = $state<FolderSummary[]>([]);
  let names = $state<Record<string, string>>({});
  let includeSub = $state<Record<string, boolean>>({});
  let error = $state('');
  let notice = $state('');
  let loaded = $state(false);

  async function load() {
    try {
      folders = await api.get<FolderSummary[]>(`/admin/libraries/${libraryId}/folders`);
      for (const folder of folders) names[folder.folder_rel] ??= folder.suggestion;
      loaded = true;
    } catch (e) {
      error = errorText(e instanceof ApiError ? e.code : 'internal');
    }
  }

  // Reload when the engine reports library changes (scan progress, thumbnails).
  $effect(() => {
    void version;
    load();
  });

  async function apply(folder: FolderSummary) {
    const name = (names[folder.folder_rel] ?? '').trim();
    if (!name) return;
    error = notice = '';
    try {
      const res = await api.post<{ tagged: number }>(`/admin/libraries/${libraryId}/folders/tag`, {
        folder_rel: folder.folder_rel,
        name,
        include_subfolders: includeSub[folder.folder_rel] ?? false,
      });
      notice = f.applied(res.tagged, name);
      await load();
    } catch (e) {
      error = errorText(e instanceof ApiError ? e.code : 'internal');
    }
  }

  const depth = (rel: string) => (rel ? rel.split('\\').length - 1 : 0);
  const leaf = (rel: string) => (rel ? rel.split('\\').pop() : f.root);
</script>

<div class="folders">
  <div class="head">
    <h2>{f.title}</h2>
    <span class="muted hint">{f.hint}</span>
  </div>
  {#if error}<div class="alert error">{error}</div>{/if}
  {#if notice}<div class="alert success">{notice}</div>{/if}

  {#if loaded && folders.length === 0}
    <div class="muted">{f.empty}</div>
  {/if}

  {#each folders as folder (folder.folder_rel)}
    <div class="folder" style:padding-left={`${12 + depth(folder.folder_rel) * 18}px`}>
      <div class="thumbs">
        {#each folder.sample_media_ids as id (id)}
          <img src={thumbUrl(id)} alt="" loading="lazy" />
        {:else}
          <div class="ph"></div>
        {/each}
      </div>
      <div class="info">
        <div class="title">
          <IconFolder size={15} class="folder-icon" />
          <strong title={folder.folder_rel}>{leaf(folder.folder_rel)}</strong>
          <span class="muted">· {folder.media_count} {f.items}</span>
        </div>
        <div class="tags">
          {#each folder.tags as t}<span class="badge accent tag"><IconTag size={12} />{t}</span>{/each}
        </div>
      </div>
      <div class="apply">
        <input class="input" bind:value={names[folder.folder_rel]} placeholder={f.eventName} />
        <label class="check small">
          <input type="checkbox" bind:checked={includeSub[folder.folder_rel]} />
          {f.includeSub}
        </label>
        <button class="btn sm" onclick={() => apply(folder)}><IconTag size={15} />{f.apply}</button>
      </div>
    </div>
  {/each}
</div>

<style>
  .folders {
    display: grid;
    gap: 6px;
    padding: 12px 14px;
    background: var(--surface-2);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 4px;
  }
  .hint {
    font-size: 12px;
  }
  .folder {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 6px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .thumbs {
    display: flex;
    gap: 3px;
    width: 180px;
  }
  .thumbs img,
  .ph {
    width: 42px;
    height: 42px;
    object-fit: cover;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .info {
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .title :global(.folder-icon) {
    color: var(--warning);
    flex: none;
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .tags {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .apply {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .apply .input {
    width: 220px;
    height: 28px;
  }
  .small {
    font-size: 12px;
    white-space: nowrap;
  }
</style>
