<script lang="ts">
  import { api, ApiError, type Tag } from '../lib/api';
  import { catalog } from '../lib/catalog.svelte';
  import { tr, errorText } from '../lib/i18n/tr';
  import { IconEdit, IconGitMerge, IconPhoto, IconPlus, IconTrash } from '../lib/icons';
  import Modal from '../lib/Modal.svelte';
  import { link } from '../lib/router.svelte';
  import TagInput from '../lib/TagInput.svelte';

  catalog.ensure();

  const t = tr.tags;
  let typeId = $state<number>(1);
  let tags = $state<Tag[]>([]);
  let filterText = $state('');
  let newName = $state('');
  let error = $state('');
  let notice = $state('');

  let editing = $state<{ tag: Tag; name: string; parent: string } | null>(null);
  let merging = $state<Tag | null>(null);
  let newType = $state<{ id: number | null; name: string; color: string } | null>(null);

  async function load() {
    try {
      tags = await api.get<Tag[]>(`/admin/tags?type=${typeId}`);
    } catch (e) {
      fail(e);
    }
  }

  $effect(() => {
    void typeId;
    load();
  });

  function fail(e: unknown) {
    error = errorText(e instanceof ApiError ? e.code : 'internal');
  }

  async function after(message = '') {
    error = '';
    notice = message;
    await Promise.all([load(), catalog.reloadTagTypes()]);
  }

  async function add() {
    if (!newName.trim()) return;
    try {
      await api.post('/admin/tags', { type_id: typeId, name: newName });
      newName = '';
      await after();
    } catch (e) {
      fail(e);
    }
  }

  async function saveEdit() {
    if (!editing) return;
    try {
      await api.put(`/admin/tags/${editing.tag.id}`, {
        name: editing.name,
        parent_id: editing.parent ? Number(editing.parent) : null,
      });
      editing = null;
      await after();
    } catch (e) {
      fail(e);
    }
  }

  async function merge(target: Tag) {
    if (!merging) return;
    try {
      await api.post(`/admin/tags/${merging.id}/merge`, { target_id: target.id });
      merging = null;
      await after(t.mergeDone);
    } catch (e) {
      fail(e);
    }
  }

  async function remove(tag: Tag) {
    if (!confirm(t.deleteConfirm(tag.name, tag.media_count))) return;
    try {
      await api.del(`/admin/tags/${tag.id}`);
      await after();
    } catch (e) {
      fail(e);
    }
  }

  async function saveType() {
    if (!newType) return;
    try {
      if (newType.id === null) {
        const r = await api.post<{ id: number }>('/admin/tag-types', { name: newType.name, color: newType.color });
        typeId = r.id;
      } else {
        await api.put(`/admin/tag-types/${newType.id}`, { name: newType.name, color: newType.color });
      }
      newType = null;
      await after();
    } catch (e) {
      fail(e);
    }
  }

  async function deleteType(id: number) {
    try {
      await api.del(`/admin/tag-types/${id}`);
      typeId = 1;
      await after();
    } catch (e) {
      fail(e);
    }
  }

  const shown = $derived(
    filterText.trim()
      ? tags.filter((x) => x.name.toLocaleLowerCase('tr').includes(filterText.trim().toLocaleLowerCase('tr')))
      : tags,
  );
  const currentType = $derived(catalog.typeById(typeId));
</script>

<div class="page">
  <p class="muted intro">{t.intro}</p>
  <div class="layout">
    <div class="card types">
      {#each catalog.tagTypes as tt (tt.id)}
        <button class="type" class:active={tt.id === typeId} onclick={() => (typeId = tt.id)}>
          <span class="dot" style:background={tt.color ?? 'var(--accent)'}></span>
          <span class="tname">{tt.name}</span>
          <span class="muted small">{tt.tag_count}</span>
        </button>
      {/each}
      <button class="btn sm ghost newtype" onclick={() => (newType = { id: null, name: '', color: '#9aa5b1' })}>
        <IconPlus size={15} />{t.newType}
      </button>
    </div>

    <div class="main">
      {#if error}<div class="alert error">{error}</div>{/if}
      {#if notice}<div class="alert success">{notice}</div>{/if}
      <div class="toolbar">
        <input class="input" bind:value={newName} placeholder={t.newTag} onkeydown={(e) => e.key === 'Enter' && add()} />
        <button class="btn primary" disabled={!newName.trim()} onclick={add}><IconPlus size={16} />{t.add}</button>
        <span class="spacer"></span>
        <input class="input filter" bind:value={filterText} placeholder={t.filter} />
        {#if currentType}
          <button class="btn sm ghost" onclick={() => (newType = { id: currentType.id, name: currentType.name, color: currentType.color ?? '#9aa5b1' })}>
            <IconEdit size={15} />{currentType.name}
          </button>
          {#if !currentType.is_builtin && currentType.tag_count === 0}
            <button class="btn sm ghost" onclick={() => deleteType(currentType.id)}><IconTrash size={15} /></button>
          {/if}
        {/if}
      </div>

      <div class="card">
        {#if shown.length === 0}
          <div class="empty muted">{t.empty}</div>
        {:else}
          <table class="table">
            <thead>
              <tr><th>{t.name}</th><th>{t.parent}</th><th class="num">{t.usage}</th><th></th></tr>
            </thead>
            <tbody>
              {#each shown as tag (tag.id)}
                <tr>
                  <td><strong>{tag.name}</strong></td>
                  <td class="muted">{tag.parent_name ?? t.noParent}</td>
                  <td class="num">{tag.media_count}</td>
                  <td class="actions">
                    <a class="btn sm ghost" href={`/admin/media?tags=${tag.id}`} use:link title={t.showMedia}><IconPhoto size={15} /></a>
                    <button class="btn sm ghost" onclick={() => (editing = { tag, name: tag.name, parent: tag.parent_id ? String(tag.parent_id) : '' })}>
                      <IconEdit size={15} />{tr.common.edit}
                    </button>
                    <button class="btn sm ghost" onclick={() => (merging = tag)}><IconGitMerge size={15} />{t.merge}</button>
                    <button class="btn sm ghost danger" onclick={() => remove(tag)}><IconTrash size={15} /></button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    </div>
  </div>
</div>

{#if editing}
  <Modal title={tr.common.edit} onClose={() => (editing = null)}>
    <label class="field"><span>{t.name}</span><input class="input" bind:value={editing.name} /></label>
    <label class="field">
      <span>{t.parent}</span>
      <select class="select" bind:value={editing.parent}>
        <option value="">{t.noParent}</option>
        {#each tags.filter((x) => x.id !== editing?.tag.id) as x (x.id)}<option value={String(x.id)}>{x.name}</option>{/each}
      </select>
    </label>
    {#snippet footer()}
      <button class="btn" onclick={() => (editing = null)}>{tr.common.cancel}</button>
      <button class="btn primary" onclick={saveEdit}>{tr.common.save}</button>
    {/snippet}
  </Modal>
{/if}

{#if merging}
  <Modal title={t.merge} onClose={() => (merging = null)}>
    <p class="small">{t.mergeInto(merging.name)}</p>
    <TagInput typeId={merging.type_id} onPick={merge} />
  </Modal>
{/if}

{#if newType}
  <Modal title={newType.id === null ? t.newType : tr.common.edit} onClose={() => (newType = null)}>
    <label class="field"><span>{t.typeName}</span><input class="input" bind:value={newType.name} /></label>
    <label class="field"><span>{t.color}</span><input class="input color" type="color" bind:value={newType.color} /></label>
    {#snippet footer()}
      <button class="btn" onclick={() => (newType = null)}>{tr.common.cancel}</button>
      <button class="btn primary" disabled={!newType?.name.trim()} onclick={saveType}>{tr.common.save}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .intro {
    margin: 0;
    font-size: 13px;
    max-width: 760px;
  }
  .layout {
    display: grid;
    grid-template-columns: 210px 1fr;
    gap: 16px;
    align-items: start;
  }
  .types {
    padding: 6px;
    display: grid;
    gap: 2px;
  }
  .type {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border: none;
    background: transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
  }
  .type:hover {
    background: var(--surface-2);
  }
  .type.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .tname {
    flex: 1;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .newtype {
    justify-content: flex-start;
    margin-top: 4px;
  }
  .main {
    display: grid;
    gap: 12px;
    min-width: 0;
  }
  .toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .toolbar .input {
    max-width: 280px;
  }
  .filter {
    max-width: 220px;
  }
  .spacer {
    flex: 1;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .empty {
    padding: 24px;
    text-align: center;
  }
  .small {
    font-size: 12px;
  }
  .danger:hover {
    color: var(--danger);
  }
  .color {
    padding: 2px;
    height: 34px;
  }
</style>
