<script lang="ts">
  import { api, ApiError, type Profile, type User } from '../lib/api';
  import { catalog } from '../lib/catalog.svelte';
  import { tr, errorText } from '../lib/i18n/tr';
  import { IconEdit, IconEye, IconPlus, IconTrash, IconUsersGroup } from '../lib/icons';
  import Modal from '../lib/Modal.svelte';
  import { link } from '../lib/router.svelte';

  catalog.ensure();

  const p = tr.profiles;
  let users = $state<User[]>([]);
  let error = $state('');
  let editing = $state<{ id: number | null; builtin: boolean; name: string; description: string; members: number[] } | null>(null);

  api.get<User[]>('/admin/users').then((u) => (users = u)).catch(fail);
  catalog.reloadProfiles();

  function fail(e: unknown) {
    error = errorText(e instanceof ApiError ? e.code : 'internal');
  }

  const userName = (id: number) => users.find((u) => u.id === id)?.display_name ?? `#${id}`;

  function open(profile: Profile | null) {
    error = '';
    editing = profile
      ? { id: profile.id, builtin: profile.is_builtin, name: profile.name, description: profile.description ?? '', members: [...profile.member_ids] }
      : { id: null, builtin: false, name: '', description: '', members: [] };
  }

  async function save() {
    if (!editing) return;
    try {
      let id = editing.id;
      if (id === null) {
        id = (await api.post<{ id: number }>('/admin/profiles', { name: editing.name, description: editing.description })).id;
      } else {
        await api.put(`/admin/profiles/${id}`, { name: editing.name, description: editing.description });
      }
      if (!editing.builtin) await api.put(`/admin/profiles/${id}/members`, { user_ids: editing.members });
      editing = null;
      await catalog.reloadProfiles();
    } catch (e) {
      fail(e);
    }
  }

  async function remove(profile: Profile) {
    if (!confirm(p.deleteConfirm(profile.name))) return;
    try {
      await api.del(`/admin/profiles/${profile.id}`);
      await catalog.reloadProfiles();
    } catch (e) {
      fail(e);
    }
  }

  function toggle(id: number) {
    if (!editing) return;
    editing.members = editing.members.includes(id) ? editing.members.filter((m) => m !== id) : [...editing.members, id];
  }
</script>

<div class="page">
  <div class="page-head">
    <p class="muted intro">{p.intro}</p>
    <button class="btn primary" onclick={() => open(null)}><IconPlus size={16} />{p.add}</button>
  </div>
  {#if error && !editing}<div class="alert error">{error}</div>{/if}

  <div class="cards">
    {#each catalog.profiles as profile (profile.id)}
      <div class="card profile">
        <div class="head">
          <IconUsersGroup size={20} class="picon" />
          <strong>{profile.name}</strong>
          {#if profile.is_builtin}<span class="badge accent">{p.builtin}</span>{/if}
        </div>
        {#if profile.description}<div class="muted small">{profile.description}</div>{/if}
        <div class="members">
          {#if profile.is_builtin}
            <span class="badge">{p.everyone}</span>
          {:else}
            {#each profile.member_ids as id}<span class="badge">{userName(id)}</span>{:else}<span class="muted small">—</span>{/each}
          {/if}
        </div>
        <div class="foot">
          <span class="muted small">{p.visible(profile.visible_media)}</span>
          <span class="spacer"></span>
          <a class="btn sm ghost" href={`/admin/media?profile=${profile.id}`} use:link><IconEye size={15} />{p.showMedia}</a>
          <button class="btn sm ghost" onclick={() => open(profile)}><IconEdit size={15} />{p.edit}</button>
          {#if !profile.is_builtin}
            <button class="btn sm ghost danger" onclick={() => remove(profile)} title={p.delete}><IconTrash size={15} /></button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
</div>

{#if editing}
  <Modal title={editing.id === null ? p.add : p.edit} onClose={() => (editing = null)}>
    {#if error}<div class="alert error">{error}</div>{/if}
    <label class="field"><span>{p.name}</span><input class="input" bind:value={editing.name} /></label>
    <label class="field"><span>{p.description}</span><input class="input" bind:value={editing.description} /></label>
    <div class="field">
      <span>{p.members}</span>
      {#if editing.builtin}
        <div class="muted small">{p.everyone}</div>
      {:else}
        <div class="userlist">
          {#each users as u (u.id)}
            <label class="check">
              <input type="checkbox" checked={editing.members.includes(u.id)} onchange={() => toggle(u.id)} />
              {u.display_name} <span class="muted small">({u.username})</span>
            </label>
          {/each}
        </div>
      {/if}
    </div>
    {#snippet footer()}
      <button class="btn" onclick={() => (editing = null)}>{tr.common.cancel}</button>
      <button class="btn primary" disabled={!editing?.name.trim()} onclick={save}>{tr.common.save}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .intro {
    margin: 0;
    font-size: 13px;
    max-width: 760px;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
  }
  .profile {
    padding: 14px 16px;
    display: grid;
    gap: 8px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .head :global(.picon) {
    color: var(--accent);
  }
  .members {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 2px;
    border-top: 1px solid var(--border);
    padding-top: 8px;
  }
  .spacer {
    flex: 1;
  }
  .small {
    font-size: 12px;
  }
  .danger:hover {
    color: var(--danger);
  }
  .userlist {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 6px;
    max-height: 260px;
    overflow: auto;
  }
</style>
