<script lang="ts">
  import { api, ApiError, type Album, type ClientPage } from '../lib/api';
  import { tr, errorText, dateRange } from '../lib/i18n/tr';
  import { IconChevronRight, IconHome } from '../lib/icons';
  import { link } from '../lib/router.svelte';
  import AlbumCard from './AlbumCard.svelte';
  import PhotoGrid from './PhotoGrid.svelte';

  let { id }: { id: number } = $props();

  let albums = $state<Album[]>([]);
  let error = $state('');

  api
    .get<Album[]>('/albums')
    .then((a) => (albums = a))
    .catch((e) => (error = errorText(e instanceof ApiError ? e.code : 'internal')));

  const album = $derived(albums.find((a) => a.id === id));
  const children = $derived(albums.filter((a) => a.parent_id === id));
  const trail = $derived.by(() => {
    const out: Album[] = [];
    let cur = album;
    while (cur?.parent_id) {
      cur = albums.find((a) => a.id === cur!.parent_id);
      if (cur) out.unshift(cur);
    }
    return out;
  });

  const load = (offset: number) => api.get<ClientPage>(`/albums/${id}/media?offset=${offset}`);
</script>

<div class="page wide">
  <nav class="crumbs muted">
    <a href="/" use:link><IconHome size={15} />{tr.album.home}</a>
    {#each trail as a (a.id)}
      <IconChevronRight size={14} /><a href={`/album/${a.id}`} use:link>{a.name}</a>
    {/each}
  </nav>

  {#if error}
    <div class="alert error">{error}</div>
  {:else if album}
    <div class="title">
      <h1>{album.name}</h1>
      <span class="muted">
        {album.total_count} {tr.album.items}{#if album.date_from}{' · '}{dateRange(album.date_from, album.date_to)}{/if}
      </span>
      {#if album.description}<p class="muted desc">{album.description}</p>{/if}
    </div>

    {#if children.length}
      <section>
        <h2>{tr.album.subAlbums}</h2>
        <div class="albums">
          {#each children as c (c.id)}
            <AlbumCard title={c.name} count={c.total_count} cover={c.cover_media_id} from={c.date_from} to={c.date_to} unit={tr.album.items} href={`/album/${c.id}`} />
          {/each}
        </div>
      </section>
    {/if}

    {#if album.media_count > 0}
      <PhotoGrid {load} emptyText={tr.album.empty} />
    {:else if !children.length}
      <div class="card empty muted">{tr.album.empty}</div>
    {/if}
  {:else if albums.length}
    <div class="alert error">{tr.errors.not_found}</div>
  {/if}
</div>

<style>
  .wide {
    max-width: 1400px;
    margin: 0 auto;
    gap: 18px;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .crumbs a {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-muted);
  }
  .crumbs a:hover {
    color: var(--accent);
  }
  .title {
    display: grid;
    gap: 2px;
  }
  .desc {
    margin: 4px 0 0;
  }
  section {
    display: grid;
    gap: 10px;
  }
  .albums {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 12px;
  }
  .empty {
    padding: 28px;
    text-align: center;
  }
</style>
