<script lang="ts">
  // Media visibility: inherit from categories / private / selected profiles.
  import { catalog } from './catalog.svelte';
  import { tr } from './i18n/tr';

  type Mode = 'inherit' | 'private' | 'profiles';
  let {
    mode = $bindable<Mode>('inherit'),
    profileIds = $bindable<number[]>([]),
    showInherit = true,
  }: { mode?: Mode; profileIds?: number[]; showInherit?: boolean } = $props();

  const v = tr.visibility;

  function toggle(id: number) {
    profileIds = profileIds.includes(id) ? profileIds.filter((p) => p !== id) : [...profileIds, id];
  }
</script>

<div class="picker">
  {#if showInherit}
    <label class="opt">
      <input type="radio" bind:group={mode} value="inherit" />
      <span><strong>{v.inherit}</strong><small class="muted">{v.inheritHint}</small></span>
    </label>
  {/if}
  <label class="opt">
    <input type="radio" bind:group={mode} value="private" />
    <span><strong>{v.private}</strong><small class="muted">{v.privateHint}</small></span>
  </label>
  <label class="opt">
    <input type="radio" bind:group={mode} value="profiles" />
    <span><strong>{v.profiles}</strong><small class="muted">{v.profilesHint}</small></span>
  </label>
  {#if mode === 'profiles'}
    <div class="profiles">
      {#each catalog.profiles as p (p.id)}
        <label class="check">
          <input type="checkbox" checked={profileIds.includes(p.id)} onchange={() => toggle(p.id)} />
          {p.name}
          <span class="muted small">({p.is_builtin ? tr.profiles.everyone : p.member_ids.length})</span>
        </label>
      {/each}
    </div>
  {/if}
</div>

<style>
  .picker {
    display: grid;
    gap: 6px;
  }
  .opt {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    cursor: pointer;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
  }
  .opt:hover {
    background: var(--surface-2);
  }
  .opt input {
    margin-top: 3px;
    accent-color: var(--accent);
  }
  .opt span {
    display: grid;
  }
  small {
    font-size: 12px;
  }
  .profiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 6px;
    padding: 4px 8px 4px 30px;
  }
  .small {
    font-size: 12px;
  }
</style>
