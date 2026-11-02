<script lang="ts">
  // Kişiler: suggestions to confirm, named persons, unnamed face groups.
  import { SvelteSet } from 'svelte/reactivity';
  import {
    api,
    ApiError,
    faceUrl,
    type FaceCluster,
    type FaceRef,
    type FaceStatus,
    type PersonSummary,
    type Tag,
  } from '../lib/api';
  import { catalog } from '../lib/catalog.svelte';
  import { engine } from '../lib/engine.svelte';
  import { tr, errorText } from '../lib/i18n/tr';
  import { IconAlertTriangle, IconCheck, IconEyeOff, IconFaceId, IconPhoto, IconX } from '../lib/icons';
  import Modal from '../lib/Modal.svelte';
  import { link } from '../lib/router.svelte';
  import TagInput from '../lib/TagInput.svelte';
  import EngineStatus from './EngineStatus.svelte';

  catalog.ensure();

  const p = tr.persons;
  let status = $state<FaceStatus | null>(null);
  let persons = $state<PersonSummary[]>([]);
  let clusters = $state<FaceCluster[]>([]);
  let suggestions = $state<{ person: PersonSummary; faces: FaceRef[] }[]>([]);
  let singles = $state(false);
  let error = $state('');
  let busy = $state(false);
  // Faces marked as wrong in suggestions / in the person dialog.
  const marked = new SvelteSet<number>();
  let opened = $state<{ person: PersonSummary; faces: FaceRef[] } | null>(null);

  const personType = $derived(catalog.typeByCode('person')?.id ?? 1);

  function fail(e: unknown) {
    error = errorText(e instanceof ApiError ? e.code : 'internal');
  }

  async function load() {
    try {
      const [s, ps, cs] = await Promise.all([
        api.get<FaceStatus>('/admin/faces/status'),
        api.get<PersonSummary[]>('/admin/faces/persons'),
        api.get<FaceCluster[]>(`/admin/faces/clusters?min=${singles ? 1 : 2}`),
      ]);
      status = s;
      persons = ps;
      clusters = cs;
      suggestions = await Promise.all(
        ps
          .filter((x) => x.suggested > 0)
          .map(async (person) => ({
            person,
            faces: await api.get<FaceRef[]>(`/admin/faces/persons/${person.tag_id}?status=suggested`),
          })),
      );
    } catch (e) {
      fail(e);
    }
  }

  // Reload after every finished engine job and when the filter changes.
  $effect(() => {
    void engine.snap?.libraries_version;
    void singles;
    load();
  });

  async function act(fn: () => Promise<unknown>) {
    busy = true;
    error = '';
    try {
      await fn();
      marked.clear();
      await load();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  const scan = () => act(() => api.post('/admin/faces/scan'));
  const nameCluster = (c: FaceCluster, tag: Tag) => act(() => api.post(`/admin/faces/clusters/${c.cluster_id}/name`, { tag_id: tag.id }));
  const ignoreCluster = (c: FaceCluster) => act(() => api.post(`/admin/faces/clusters/${c.cluster_id}/ignore`));

  function resolveSuggestions(s: { person: PersonSummary; faces: FaceRef[] }) {
    const wrong = s.faces.filter((f) => marked.has(f.id)).map((f) => f.id);
    const right = s.faces.filter((f) => !marked.has(f.id)).map((f) => f.id);
    return act(async () => {
      if (wrong.length) await api.post('/admin/faces/reject', { face_ids: wrong });
      if (right.length) await api.post('/admin/faces/confirm', { face_ids: right, tag_id: s.person.tag_id });
    });
  }

  async function openPerson(person: PersonSummary) {
    marked.clear();
    try {
      opened = { person, faces: await api.get<FaceRef[]>(`/admin/faces/persons/${person.tag_id}`) };
    } catch (e) {
      fail(e);
    }
  }

  async function removeFromPerson() {
    const ids = [...marked];
    await act(() => api.post('/admin/faces/reject', { face_ids: ids }));
    if (opened) await openPerson(opened.person);
  }

  const toggle = (id: number) => (marked.has(id) ? marked.delete(id) : marked.add(id));
</script>

<div class="page">
  <div class="page-head">
    <p class="muted intro">{p.intro}</p>
    <button class="btn primary" disabled={busy || !status?.available || !!engine.running} onclick={scan}>
      <IconFaceId size={16} />{p.scan}
    </button>
  </div>

  {#if status && !status.available}
    <div class="alert error with-icon">
      <IconAlertTriangle size={16} />
      <div><strong>{p.missingTitle}</strong><br />{p.missingHelp(status.models_dir)}</div>
    </div>
  {/if}
  <EngineStatus />
  {#if error}<div class="alert error">{error}</div>{/if}
  {#if status?.available}<div class="muted small">{p.stats(status)}</div>{/if}

  {#if suggestions.length}
    <section>
      <h2>{p.suggestions}</h2>
      <p class="muted small hint">{p.suggestionsHint}</p>
      {#each suggestions as s (s.person.tag_id)}
        {@const wrong = s.faces.filter((f) => marked.has(f.id)).length}
        <div class="card block">
          <div class="block-head">
            <strong>{p.isThis(s.person.name)}</strong>
            <span class="spacer"></span>
            <button class="btn sm primary" disabled={busy} onclick={() => resolveSuggestions(s)}>
              <IconCheck size={15} />{wrong ? p.confirmOthers(s.faces.length - wrong) : p.confirm}
            </button>
          </div>
          <div class="faces">
            {#each s.faces as f (f.id)}
              <button class="face" class:wrong={marked.has(f.id)} onclick={() => toggle(f.id)} title={p.notThis}>
                <img src={faceUrl(f.id)} alt="" loading="lazy" />
                {#if marked.has(f.id)}<span class="mark"><IconX size={18} stroke={3} /></span>{/if}
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </section>
  {/if}

  {#if persons.length}
    <section>
      <h2>{p.people}</h2>
      <div class="people">
        {#each persons as person (person.tag_id)}
          <button class="card person" onclick={() => openPerson(person)}>
            {#if person.cover_face_id}<img src={faceUrl(person.cover_face_id)} alt="" />{:else}<div class="noface"></div>{/if}
            <strong>{person.name}</strong>
            <span class="muted small">{person.confirmed} {p.faces}{#if person.suggested}{' · '}{person.suggested} ?{/if}</span>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  {#if status?.available}
    <section>
      <div class="section-head">
        <h2>{p.groups}</h2>
        <span class="spacer"></span>
        <label class="check small"><input type="checkbox" bind:checked={singles} />{p.singles}</label>
      </div>
      <p class="muted small hint">{p.groupsHint}</p>
      {#if clusters.length === 0}
        <div class="card empty muted">{status.faces ? p.noGroups : p.none}</div>
      {/if}
      {#each clusters as c (c.cluster_id)}
        <div class="card block">
          <div class="faces">
            {#each c.faces as f (f.id)}<div class="face static"><img src={faceUrl(f.id)} alt="" loading="lazy" /></div>{/each}
          </div>
          <div class="block-foot">
            <span class="muted small">{p.inGroup(c.count, c.media_count)}</span>
            <span class="spacer"></span>
            <div class="name-input"><TagInput allowCreate typeId={personType} placeholder={p.name} onPick={(t: Tag) => nameCluster(c, t)} /></div>
            <button class="btn sm ghost" disabled={busy} onclick={() => ignoreCluster(c)}><IconEyeOff size={15} />{p.ignore}</button>
          </div>
        </div>
      {/each}
    </section>
  {/if}
</div>

{#if opened}
  <Modal title={opened.person.name} width={760} onClose={() => (opened = null)}>
    <div class="faces wrap">
      {#each opened.faces as f (f.id)}
        <button class="face" class:wrong={marked.has(f.id)} onclick={() => toggle(f.id)}>
          <img src={faceUrl(f.id)} alt="" loading="lazy" />
          {#if marked.has(f.id)}<span class="mark"><IconX size={18} stroke={3} /></span>{/if}
        </button>
      {/each}
    </div>
    {#snippet footer()}
      <a class="btn" href={`/admin/media?tags=${opened?.person.tag_id}`} use:link onclick={() => (opened = null)}>
        <IconPhoto size={15} />{p.showPhotos}
      </a>
      <span class="spacer"></span>
      <button class="btn" disabled={marked.size === 0 || busy} onclick={removeFromPerson}><IconX size={15} />{p.removeSelected}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .intro {
    margin: 0;
    font-size: 13px;
    max-width: 760px;
  }
  .small {
    font-size: 12px;
  }
  .hint {
    margin: -4px 0 0;
  }
  section {
    display: grid;
    gap: 10px;
  }
  .section-head,
  .block-head,
  .block-foot {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
  .block {
    padding: 10px 12px;
    display: grid;
    gap: 10px;
  }
  .faces {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding-bottom: 2px;
  }
  .faces.wrap {
    flex-wrap: wrap;
  }
  .face {
    position: relative;
    flex: none;
    width: 76px;
    height: 76px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface-2);
    cursor: pointer;
  }
  .face.static {
    cursor: default;
  }
  .face img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .face.wrong {
    border-color: var(--danger);
  }
  .face.wrong img {
    opacity: 0.45;
  }
  .mark {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--danger);
  }
  .people {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 10px;
  }
  .person {
    display: grid;
    justify-items: center;
    gap: 4px;
    padding: 12px 8px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .person:hover {
    border-color: var(--accent);
  }
  .person img,
  .noface {
    width: 84px;
    height: 84px;
    border-radius: 50%;
    object-fit: cover;
    background: var(--surface-2);
  }
  .name-input {
    width: 260px;
  }
  .empty {
    padding: 24px;
    text-align: center;
  }
</style>
