<script lang="ts">
  // Running job with live progress, or a summary of the last finished one.
  import { api } from '../lib/api';
  import { engine } from '../lib/engine.svelte';
  import { tr } from '../lib/i18n/tr';
  import { IconPlayerStop } from '../lib/icons';

  const e = tr.engine;
  const cur = $derived(engine.snap?.current ?? null);
  const last = $derived(engine.snap?.last_finished ?? null);
  const percent = $derived(cur && cur.total > 0 ? Math.min(100, Math.round((cur.done / cur.total) * 100)) : null);
</script>

{#if cur}
  <div class="card job">
    <div class="row">
      <strong>
        {cur.kind === 'scan' ? `${e.scanning}: ${cur.library_name ?? ''}` : cur.kind === 'faces' ? e.facesJob : e.thumbsJob}
      </strong>
      <span class="muted">· {e.phases[cur.phase] ?? cur.phase}</span>
      <span class="spacer"></span>
      {#if (engine.snap?.queued ?? 0) > 0}<span class="badge">{e.queued(engine.snap!.queued)}</span>{/if}
      <button class="btn sm" onclick={() => api.post(`/admin/jobs/${cur.job_id}/cancel`)}><IconPlayerStop size={15} />{e.stop}</button>
    </div>
    <div class="bar" class:indeterminate={percent === null}>
      <div style:width={percent === null ? '30%' : `${percent}%`}></div>
    </div>
    <div class="row stats muted">
      {#if cur.phase === 'listing'}
        <span>{cur.listed} {e.listed}</span>
      {:else}
        <span>{cur.done} / {cur.total}{percent !== null ? ` (%${percent})` : ''}</span>
        {#if cur.kind === 'scan'}
          <span>· {cur.new_media} {e.newMedia}</span>
          <span>· {cur.existing_media} {e.existing}</span>
          <span>· {cur.unchanged} {e.unchanged}</span>
        {/if}
        {#if cur.kind === 'faces'}<span>· {cur.faces_found} {e.facesFound}</span>{/if}
        {#if cur.errors > 0}<span class="err">· {cur.errors} {e.errors}</span>{/if}
      {/if}
    </div>
    {#if cur.current}<div class="current muted" title={cur.current}>{cur.current}</div>{/if}
  </div>
{:else if last}
  {@const p = last.progress}
  <div class="alert {last.status === 'done' ? 'success' : 'error'} summary">
    <strong>{e.finished[last.status] ?? last.status}</strong>
    {#if p.library_name}<span>· {p.library_name}</span>{/if}
    {#if p.kind === 'scan' && last.status === 'done'}
      <span>· {p.listed} {e.listed}, {p.new_media} {e.newMedia}, {p.existing_media} {e.existing}, {p.missing} {e.missing}</span>
    {:else if p.kind === 'faces' && last.status === 'done'}
      <span>· {e.facesJob}: {p.done} / {p.total}, {p.faces_found} {e.facesFound}</span>
    {/if}
    {#if p.errors + p.thumbs_failed > 0}<span>· {p.errors + p.thumbs_failed} {e.errors}</span>{/if}
    {#if last.message}<span>· {tr.errors[last.message] ?? last.message}</span>{/if}
  </div>
{/if}

<style>
  .job {
    padding: 12px 16px;
    display: grid;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .spacer {
    flex: 1;
  }
  .stats {
    font-size: 12px;
  }
  .err {
    color: var(--danger);
  }
  .current {
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 0.3s;
  }
  .bar.indeterminate > div {
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  .summary {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
</style>
