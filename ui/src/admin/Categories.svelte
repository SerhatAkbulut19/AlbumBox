<script lang="ts">
  import { api, ApiError, thumbUrl } from '../lib/api';
  import { catalog, type CategoryNode } from '../lib/catalog.svelte';
  import { tr, errorText } from '../lib/i18n/tr';
  import {
    IconArrowDown,
    IconArrowUp,
    IconEdit,
    IconFolderPlus,
    IconGripVertical,
    IconLock,
    IconPhoto,
    IconPlus,
    IconShare,
    IconTrash,
  } from '../lib/icons';
  import Modal from '../lib/Modal.svelte';
  import { link } from '../lib/router.svelte';

  catalog.ensure();

  const c = tr.categories;
  let error = $state('');

  // Edit / create dialog
  let editing = $state<{ id: number | null; parent_id: number | null; name: string; description: string } | null>(null);
  // Publish dialog
  let sharing = $state<{ node: CategoryNode; profileIds: number[]; children: boolean } | null>(null);

  const tree = $derived(catalog.tree);

  function fail(e: unknown) {
    error = errorText(e instanceof ApiError ? e.code : 'internal');
  }

  async function reload() {
    await catalog.reloadCategories();
  }

  async function saveEdit() {
    if (!editing) return;
    error = '';
    try {
      if (editing.id === null) {
        await api.post('/admin/categories', { parent_id: editing.parent_id, name: editing.name, description: editing.description });
      } else {
        await api.put(`/admin/categories/${editing.id}`, { name: editing.name, description: editing.description });
      }
      editing = null;
      await reload();
    } catch (e) {
      fail(e);
    }
  }

  async function saveShares() {
    if (!sharing) return;
    error = '';
    try {
      await api.put(`/admin/categories/${sharing.node.id}/shares`, {
        profile_ids: sharing.profileIds,
        include_children: sharing.children,
      });
      sharing = null;
      await Promise.all([reload(), catalog.reloadProfiles()]);
    } catch (e) {
      fail(e);
    }
  }

  async function remove(node: CategoryNode) {
    if (!confirm(c.deleteConfirm(node.name))) return;
    try {
      await api.del(`/admin/categories/${node.id}`);
      await Promise.all([reload(), catalog.reloadProfiles()]);
    } catch (e) {
      fail(e);
    }
  }

  async function move(id: number, parent_id: number | null, sort_order?: number) {
    error = '';
    try {
      await api.put(`/admin/categories/${id}/move`, { parent_id, sort_order });
      await reload();
    } catch (e) {
      fail(e);
    }
  }

  function siblings(node: CategoryNode) {
    return tree.filter((n) => n.parent_id === node.parent_id);
  }

  function shift(node: CategoryNode, delta: -1 | 1) {
    const sib = siblings(node);
    const i = sib.findIndex((n) => n.id === node.id);
    const other = sib[i + delta];
    if (!other) return;
    // Up: take the previous sibling's slot. Down: go after the next one.
    move(node.id, node.parent_id, delta < 0 ? other.sort_order : other.sort_order + 1);
  }

  // ---- drag & drop: drop on a row = become its child, drop on the bar = top level
  let dragId = $state<number | null>(null);
  let dropTarget = $state<number | 'root' | null>(null);

  function onDrop(target: number | 'root') {
    const id = dragId;
    dragId = null;
    dropTarget = null;
    if (id === null || id === target) return;
    move(id, target === 'root' ? null : target);
  }

  const shareLabel = (n: CategoryNode) => (n.profile_ids.length ? n.profile_ids.map((p) => catalog.profileName(p)).join(', ') : null);
</script>

<div class="page">
  <div class="page-head">
    <p class="muted intro">{c.intro}</p>
    <button class="btn primary" onclick={() => (editing = { id: null, parent_id: null, name: '', description: '' })}>
      <IconPlus size={16} />{c.add}
    </button>
  </div>
  {#if error}<div class="alert error">{error}</div>{/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="root-drop muted"
    class:over={dropTarget === 'root'}
    ondragover={(e) => {
      if (dragId !== null) {
        e.preventDefault();
        dropTarget = 'root';
      }
    }}
    ondragleave={() => (dropTarget = null)}
    ondrop={() => onDrop('root')}
  >
    {dragId !== null ? c.dropRoot : c.dropHint}
  </div>

  {#if tree.length === 0}
    <div class="card empty muted">{c.empty}</div>
  {:else}
    <div class="card list">
      {#each tree as node (node.id)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="row"
          class:over={dropTarget === node.id}
          class:dragging={dragId === node.id}
          style:padding-left={`${10 + node.depth * 24}px`}
          draggable="true"
          ondragstart={(e) => {
            dragId = node.id;
            e.dataTransfer?.setData('text/plain', String(node.id));
          }}
          ondragend={() => {
            dragId = null;
            dropTarget = null;
          }}
          ondragover={(e) => {
            if (dragId !== null && dragId !== node.id) {
              e.preventDefault();
              dropTarget = node.id;
            }
          }}
          ondragleave={() => dropTarget === node.id && (dropTarget = null)}
          ondrop={(e) => {
            e.preventDefault();
            onDrop(node.id);
          }}
        >
          <span class="grip muted"><IconGripVertical size={16} /></span>
          <div class="cover">
            {#if node.cover_media_id}<img src={thumbUrl(node.cover_media_id)} alt="" draggable="false" />{/if}
          </div>
          <div class="name">
            <strong>{node.name}</strong>
            <span class="muted small">· {node.media_count} {c.items}</span>
            {#if node.description}<div class="muted small desc">{node.description}</div>{/if}
          </div>
          <div class="share">
            {#if shareLabel(node)}
              <span class="badge success"><IconShare size={12} /> {shareLabel(node)}</span>
            {:else}
              <span class="badge"><IconLock size={12} /> {c.private}</span>
            {/if}
          </div>
          <div class="actions">
            <a class="btn sm ghost" href={`/admin/media?category=${node.id}`} use:link title={c.showMedia}><IconPhoto size={15} /></a>
            <button class="btn sm ghost" title={c.up} onclick={() => shift(node, -1)}><IconArrowUp size={15} /></button>
            <button class="btn sm ghost" title={c.down} onclick={() => shift(node, 1)}><IconArrowDown size={15} /></button>
            <button class="btn sm ghost" title={c.addChild} onclick={() => (editing = { id: null, parent_id: node.id, name: '', description: '' })}>
              <IconFolderPlus size={15} />
            </button>
            <button
              class="btn sm ghost"
              title={c.edit}
              onclick={() => (editing = { id: node.id, parent_id: node.parent_id, name: node.name, description: node.description ?? '' })}
            >
              <IconEdit size={15} />
            </button>
            <button class="btn sm" onclick={() => (sharing = { node, profileIds: [...node.profile_ids], children: false })}>
              <IconShare size={15} />{c.publish}
            </button>
            <button class="btn sm ghost danger" title={c.delete} onclick={() => remove(node)}><IconTrash size={15} /></button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if editing}
  <Modal title={editing.id === null ? (editing.parent_id ? c.addChild : c.add) : c.edit} onClose={() => (editing = null)}>
    <label class="field">
      <span>{c.name}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" bind:value={editing.name} autofocus onkeydown={(e) => e.key === 'Enter' && saveEdit()} />
    </label>
    <label class="field">
      <span>{c.description}</span>
      <input class="input" bind:value={editing.description} />
    </label>
    {#snippet footer()}
      <button class="btn" onclick={() => (editing = null)}>{tr.common.cancel}</button>
      <button class="btn primary" disabled={!editing?.name.trim()} onclick={saveEdit}>{tr.common.save}</button>
    {/snippet}
  </Modal>
{/if}

{#if sharing}
  <Modal title={`${c.publish}: ${sharing.node.name}`} onClose={() => (sharing = null)}>
    <p class="muted small">{tr.profiles.intro}</p>
    <div class="profiles">
      {#each catalog.profiles as p (p.id)}
        <label class="check">
          <input
            type="checkbox"
            checked={sharing.profileIds.includes(p.id)}
            onchange={() =>
              sharing &&
              (sharing.profileIds = sharing.profileIds.includes(p.id)
                ? sharing.profileIds.filter((x) => x !== p.id)
                : [...sharing.profileIds, p.id])}
          />
          {p.name} <span class="muted small">({p.is_builtin ? tr.profiles.everyone : p.member_ids.length})</span>
        </label>
      {/each}
    </div>
    {#if sharing.profileIds.length === 0}<div class="alert">{tr.visibility.privateHint}</div>{/if}
    <label class="check"><input type="checkbox" bind:checked={sharing.children} />{tr.visibility.toChildren}</label>
    {#snippet footer()}
      <button class="btn" onclick={() => (sharing = null)}>{tr.common.cancel}</button>
      <button class="btn primary" onclick={saveShares}>{tr.common.save}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .intro {
    margin: 0;
    font-size: 13px;
    max-width: 760px;
  }
  .root-drop {
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
    padding: 6px 12px;
    font-size: 12px;
  }
  .root-drop.over {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .empty {
    padding: 28px;
    text-align: center;
  }
  .list {
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: auto 44px 1fr auto auto;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .row:last-child {
    border-bottom: none;
  }
  .row.over {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .row.dragging {
    opacity: 0.4;
  }
  .grip {
    cursor: grab;
    display: grid;
  }
  .cover {
    width: 44px;
    height: 44px;
    border-radius: 6px;
    overflow: hidden;
    background: var(--surface-2);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .name {
    min-width: 0;
  }
  .desc {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .small {
    font-size: 12px;
  }
  .share .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .actions {
    display: flex;
    gap: 2px;
    align-items: center;
  }
  .danger:hover {
    color: var(--danger);
  }
  .profiles {
    display: grid;
    gap: 6px;
  }
</style>
