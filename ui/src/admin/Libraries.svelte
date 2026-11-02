<script lang="ts">
  import { api, ApiError, type Library } from '../lib/api';
  import { engine } from '../lib/engine.svelte';
  import { tr, errorText, formatDate } from '../lib/i18n/tr';
  import EngineStatus from './EngineStatus.svelte';
  import FolderPicker from './FolderPicker.svelte';
  import LibraryFolders from './LibraryFolders.svelte';
  import { IconAlertTriangle, IconFolders, IconPlus, IconRefresh, IconScan } from '../lib/icons';

  const l = tr.libraries;
  let libs = $state<Library[]>([]);
  let loaded = $state(false);
  let picking = $state(false);
  let openFolders = $state<string | null>(null);
  let error = $state('');
  let notice = $state('');

  const version = $derived(engine.snap?.libraries_version ?? 0);

  async function load() {
    try {
      libs = await api.get<Library[]>('/admin/libraries');
      loaded = true;
    } catch (e) {
      error = errorText(e instanceof ApiError ? e.code : 'internal');
    }
  }

  // Reload whenever the engine reports a library change (plug/unplug, scan done).
  $effect(() => {
    void version;
    load();
  });

  async function scan(lib: Library) {
    error = notice = '';
    try {
      await api.post(`/admin/libraries/${lib.id}/scan`);
      notice = l.scanQueued;
    } catch (e) {
      error = errorText(e instanceof ApiError ? e.code : 'internal');
    }
  }

  function added() {
    picking = false;
    notice = l.added;
    load();
  }
</script>

<div class="page">
  <div class="page-head">
    <p class="muted intro">{l.intro}</p>
    <div class="actions">
      <button class="btn" onclick={() => api.post('/admin/libraries/refresh')}><IconRefresh size={16} />{l.refresh}</button>
      <button class="btn primary" onclick={() => (picking = !picking)}><IconPlus size={16} />{l.add}</button>
    </div>
  </div>

  {#if engine.snap && !engine.snap.ffmpeg_available}
    <div class="alert error with-icon"><IconAlertTriangle size={16} />{l.ffmpegMissing}</div>
  {/if}
  <EngineStatus />
  {#if error}<div class="alert error">{error}</div>{/if}
  {#if notice}<div class="alert success">{notice}</div>{/if}

  {#if picking}
    <FolderPicker onAdded={added} onCancel={() => (picking = false)} />
  {/if}

  {#if loaded && libs.length === 0}
    <div class="card empty muted">{l.empty}</div>
  {:else if libs.length > 0}
    <div class="card">
      <table class="table">
        <thead>
          <tr>
            <th>{l.name}</th>
            <th>{l.status}</th>
            <th class="num">{l.files}</th>
            <th class="num">{l.media}</th>
            <th class="num">{l.missing}</th>
            <th>{l.lastScan}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each libs as lib (lib.id)}
            <tr>
              <td><strong>{lib.name}</strong></td>
              <td>
                {#if lib.is_online}
                  <span class="badge success">{l.online}</span>
                  <span class="muted root">{lib.last_seen_root}</span>
                {:else}
                  <span class="badge danger">{l.offline}</span>
                {/if}
              </td>
              <td class="num">{lib.file_count}</td>
              <td class="num">{lib.media_count}</td>
              <td class="num" class:warn={lib.missing_count > 0}>{lib.missing_count}</td>
              <td class="muted">{lib.last_scan_at ? formatDate(lib.last_scan_at) : l.never}</td>
              <td class="actions">
                <button
                  class="btn sm ghost"
                  onclick={() => (openFolders = openFolders === lib.id ? null : lib.id)}
                >
                  <IconFolders size={15} />{openFolders === lib.id ? l.hideFolders : l.folders}
                </button>
                <button class="btn sm" disabled={!lib.is_online || !!engine.running} onclick={() => scan(lib)}>
                  <IconScan size={15} />{l.scan}
                </button>
              </td>
            </tr>
            {#if openFolders === lib.id}
              <tr class="folders-row">
                <td colspan="7"><LibraryFolders libraryId={lib.id} {version} /></td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .intro {
    margin: 0;
    font-size: 13px;
    max-width: 760px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .empty {
    padding: 28px;
    text-align: center;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .warn {
    color: var(--warning);
  }
  .root {
    font-size: 12px;
    margin-left: 6px;
  }
  .folders-row > td {
    padding: 0;
  }
</style>
