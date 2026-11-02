<script lang="ts">
  import { api, ApiError, type Role, type User } from '../lib/api';
  import { session } from '../lib/session.svelte';
  import { tr, errorText, formatDate } from '../lib/i18n/tr';
  import { IconEdit, IconKey, IconUserPlus } from '../lib/icons';
  import { catalog } from '../lib/catalog.svelte';

  catalog.reloadProfiles();
  const profilesOf = (id: number) => catalog.profiles.filter((p) => !p.is_builtin && p.member_ids.includes(id));

  let users = $state<User[]>([]);
  let error = $state('');
  let notice = $state('');

  // Inline editor: 'new' for the create form, a user id for editing, null when closed.
  let editing = $state<'new' | number | null>(null);
  let form = $state({ username: '', display_name: '', password: '', role: 'member' as Role, is_active: true });
  let busy = $state(false);

  // Password reset for one row.
  let resetFor = $state<number | null>(null);
  let resetPassword = $state('');

  function fail(e: unknown) {
    error = errorText(e instanceof ApiError ? e.code : 'internal');
  }

  async function load() {
    try {
      users = await api.get<User[]>('/admin/users');
    } catch (e) {
      fail(e);
    }
  }
  load();

  function openNew() {
    form = { username: '', display_name: '', password: '', role: 'member', is_active: true };
    editing = 'new';
    resetFor = null;
    error = notice = '';
  }

  function openEdit(u: User) {
    form = { username: u.username, display_name: u.display_name, password: '', role: u.role, is_active: u.is_active };
    editing = u.id;
    resetFor = null;
    error = notice = '';
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = notice = '';
    try {
      if (editing === 'new') {
        await api.post('/admin/users', {
          username: form.username,
          display_name: form.display_name,
          password: form.password,
          role: form.role,
        });
        notice = tr.users.created;
      } else if (editing !== null) {
        await api.put(`/admin/users/${editing}`, {
          display_name: form.display_name,
          role: form.role,
          is_active: form.is_active,
        });
        notice = tr.users.updated;
      }
      editing = null;
      await load();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  async function savePassword(e: SubmitEvent) {
    e.preventDefault();
    if (resetFor === null) return;
    busy = true;
    error = notice = '';
    try {
      await api.put(`/admin/users/${resetFor}/password`, { password: resetPassword });
      notice = tr.user.passwordChanged;
      resetFor = null;
      resetPassword = '';
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <p class="muted intro">{tr.users.intro}</p>
    <button class="btn primary" onclick={openNew}><IconUserPlus size={16} />{tr.users.add}</button>
  </div>

  {#if error}<div class="alert error">{error}</div>{/if}
  {#if notice}<div class="alert success">{notice}</div>{/if}

  {#if editing !== null}
    <form class="card editor" onsubmit={save}>
      <label class="field">
        <span>{tr.login.username}</span>
        <input class="input" bind:value={form.username} disabled={editing !== 'new'} autocomplete="off" required />
      </label>
      <label class="field">
        <span>{tr.user.displayName}</span>
        <input class="input" bind:value={form.display_name} required />
      </label>
      {#if editing === 'new'}
        <label class="field">
          <span>{tr.login.password}</span>
          <input class="input" type="password" bind:value={form.password} autocomplete="new-password" required />
        </label>
      {/if}
      <label class="field">
        <span>{tr.user.role}</span>
        <select class="select" bind:value={form.role}>
          <option value="member">{tr.user.roles.member}</option>
          <option value="admin">{tr.user.roles.admin}</option>
        </select>
      </label>
      {#if editing !== 'new'}
        <label class="check active-check">
          <input type="checkbox" bind:checked={form.is_active} />
          {tr.user.active}
        </label>
      {/if}
      <div class="editor-actions">
        <button class="btn" type="button" onclick={() => (editing = null)}>{tr.common.cancel}</button>
        <button class="btn primary" type="submit" disabled={busy}>{tr.common.save}</button>
      </div>
    </form>
  {/if}

  <div class="card">
    <table class="table">
      <thead>
        <tr>
          <th>{tr.user.displayName}</th>
          <th>{tr.login.username}</th>
          <th>{tr.user.role}</th>
          <th>{tr.user.active}</th>
          <th>{tr.nav.profiles}</th>
          <th>{tr.user.lastLogin}</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each users as u (u.id)}
          <tr>
            <td>{u.display_name}{#if u.id === session.user?.id}<span class="muted self">(siz)</span>{/if}</td>
            <td class="muted">{u.username}</td>
            <td><span class="badge {u.role === 'admin' ? 'accent' : ''}">{tr.user.roles[u.role]}</span></td>
            <td>
              <span class="badge {u.is_active ? 'success' : 'danger'}">
                {u.is_active ? tr.user.active : tr.user.passive}
              </span>
            </td>
            <td>{#each profilesOf(u.id) as p (p.id)}<span class="badge">{p.name}</span> {:else}<span class="muted">—</span>{/each}</td>
            <td class="muted">{formatDate(u.last_login_at)}</td>
            <td class="actions">
              <button class="btn sm ghost" onclick={() => openEdit(u)}><IconEdit size={15} />{tr.common.edit}</button>
              <button
                class="btn sm ghost"
                onclick={() => {
                  resetFor = resetFor === u.id ? null : u.id;
                  resetPassword = '';
                  editing = null;
                }}><IconKey size={15} />{tr.user.resetPassword}</button
              >
            </td>
          </tr>
          {#if resetFor === u.id}
            <tr class="reset-row">
              <td colspan="7">
                <form class="reset" onsubmit={savePassword}>
                  <span class="muted">{u.display_name} — {tr.user.newPassword}</span>
                  <input class="input" type="password" bind:value={resetPassword} autocomplete="new-password" required />
                  <button class="btn sm" type="button" onclick={() => (resetFor = null)}>{tr.common.cancel}</button>
                  <button class="btn sm primary" type="submit" disabled={busy}>{tr.common.save}</button>
                </form>
              </td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .self {
    margin-left: 4px;
  }
  .intro {
    margin: 0;
    font-size: 13px;
  }
  .editor {
    padding: 14px 16px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 12px;
    align-items: end;
  }
  .active-check {
    height: var(--control-h);
  }
  .editor-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    grid-column: 1 / -1;
  }
  .reset-row td {
    background: var(--surface-2);
  }
  .reset {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .reset .input {
    max-width: 220px;
    height: 26px;
  }
</style>
