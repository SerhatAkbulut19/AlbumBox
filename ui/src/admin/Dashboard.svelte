<script lang="ts">
  import { api, ApiError, type AdminStats } from '../lib/api';
  import { link } from '../lib/router.svelte';
  import { tr, errorText } from '../lib/i18n/tr';
  import { engine } from '../lib/engine.svelte';
  import EngineStatus from './EngineStatus.svelte';
  import { IconDatabase } from '../lib/icons';

  let stats = $state<AdminStats | null>(null);
  let error = $state('');

  // Reload when library state changes (scan finished, disk plugged in).
  $effect(() => {
    void engine.snap?.libraries_version;
    api
      .get<AdminStats>('/admin/stats')
      .then((s) => (stats = s))
      .catch((e) => (error = errorText(e instanceof ApiError ? e.code : 'internal')));
  });

  const d = tr.dashboard;
</script>

<div class="page">
  <EngineStatus />
  {#if error}
    <div class="alert error">{error}</div>
  {:else if !stats}
    <div class="muted">{tr.common.loading}</div>
  {:else}
    <div class="tiles">
      <div class="card tile">
        <span class="label">{d.media}</span>
        <span class="value">{stats.media}</span>
        <span class="sub muted">{stats.photos} {d.photos} · {stats.videos} {d.videos}</span>
      </div>
      <div class="card tile">
        <span class="label">{d.published}</span>
        <span class="value">{stats.published_media}</span>
        <span class="sub muted">{stats.media - stats.published_media} {d.privateMedia}</span>
      </div>
      <div class="card tile">
        <span class="label">{d.persons}</span>
        <span class="value">{stats.persons}</span>
        <span class="sub muted">{stats.pending_face_suggestions} {d.faceSuggestions}</span>
      </div>
      <div class="card tile">
        <span class="label">{d.categories}</span>
        <span class="value">{stats.categories}</span>
        <span class="sub muted">{stats.tags} {d.tags}</span>
      </div>
      <div class="card tile">
        <span class="label">{d.users}</span>
        <span class="value">{stats.users}</span>
        <span class="sub muted">{stats.profiles} {d.profiles}</span>
      </div>
      <div class="card tile">
        <span class="label">{d.libraries}</span>
        <span class="value">{stats.libraries}</span>
        <span class="sub muted">{stats.libraries_online} {d.online}</span>
      </div>
    </div>
    {#if stats.media === 0}
      <div class="card hint">
        <span>{d.empty}</span>
        <a class="btn sm" href="/admin/libraries" use:link><IconDatabase size={15} />{tr.nav.libraries}</a>
      </div>
    {/if}
  {/if}
</div>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 12px;
  }
  .tile {
    padding: 14px 16px;
    display: grid;
    gap: 2px;
  }
  .label {
    font-size: 12px;
    color: var(--text-muted);
  }
  .value {
    font-size: 24px;
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
  }
  .hint {
    padding: 14px 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
</style>
