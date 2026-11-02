<script lang="ts">
  import { api, ApiError, type Home } from '../lib/api';
  import { tr, errorText } from '../lib/i18n/tr';
  import { link } from '../lib/router.svelte';
  import AlbumCard from './AlbumCard.svelte';
  import PhotoGrid from './PhotoGrid.svelte';

  let home = $state<Home | null>(null);
  let error = $state('');

  api
    .get<Home>('/home')
    .then((h) => (home = h))
    .catch((e) => (error = errorText(e instanceof ApiError ? e.code : 'internal')));

  const h = tr.home;
</script>

<div class="page wide">
  {#if error}
    <div class="alert error">{error}</div>
  {:else if !home}
    <div class="muted">{tr.common.loading}</div>
  {:else}
    <div class="hello">
      <h1>{h.welcome}, {home.display_name}</h1>
      <span class="muted">{h.summary(home.visible_media, home.album_count)}</span>
      <span class="muted small">{h.searchHint}</span>
    </div>

    {#if home.visible_media === 0}
      <div class="card empty muted">{h.empty}</div>
    {:else}
      {#if home.albums.length}
        <section>
          <h2>{h.albums}</h2>
          <div class="albums">
            {#each home.albums as a (a.id)}
              <AlbumCard
                title={a.name}
                count={a.total_count}
                cover={a.cover_media_id}
                from={a.date_from}
                to={a.date_to}
                unit={tr.album.items}
                href={`/album/${a.id}`}
              />
            {/each}
          </div>
        </section>
      {/if}

      {#if home.years.length}
        <section>
          <h2>{h.years}</h2>
          <div class="years">
            {#each home.years as [year, count] (year)}
              <a class="year" href={`/search?year=${year}&group=event`} use:link>
                <strong>{year}</strong><span class="muted">{count}</span>
              </a>
            {/each}
          </div>
        </section>
      {/if}

      <section>
        <h2>{h.recent}</h2>
        <PhotoGrid load={async () => ({ total: home!.recent.length, offset: 0, items: home!.recent })} />
      </section>
    {/if}
  {/if}
</div>

<style>
  .wide {
    max-width: 1400px;
    margin: 0 auto;
    gap: 24px;
  }
  .hello {
    display: grid;
    gap: 2px;
  }
  .small {
    font-size: 12px;
  }
  section {
    display: grid;
    gap: 10px;
  }
  .albums {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 14px;
  }
  .years {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .year {
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
    padding: 8px 14px;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
  }
  .year:hover {
    border-color: var(--accent);
  }
  .year span {
    font-size: 12px;
  }
  .empty {
    padding: 28px;
    text-align: center;
  }
</style>
