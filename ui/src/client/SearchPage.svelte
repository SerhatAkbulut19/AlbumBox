<script lang="ts">
  // Grouped search: "köy, nişan" → one card per event (or year), facets on the
  // left to narrow down. Clicking a card shows that group's photos.
  import { api, ApiError, type ClientPage, type Facet, type SearchResult } from '../lib/api';
  import { tr, errorText } from '../lib/i18n/tr';
  import { IconChevronLeft, IconX } from '../lib/icons';
  import { navigate, router } from '../lib/router.svelte';
  import AlbumCard from './AlbumCard.svelte';
  import PhotoGrid from './PhotoGrid.svelte';

  const s = tr.search;
  const params = $derived(new URLSearchParams(router.search));
  const tagIds = $derived((params.get('tags') ?? '').split(',').map(Number).filter((n) => n > 0));
  const group = $derived(params.get('group') === 'year' ? 'year' : 'event');
  const key = $derived(params.get('key'));

  let result = $state<SearchResult | null>(null);
  let error = $state('');
  // Names of tags picked from facets (kept across searches).
  let tagNames = $state<Record<number, string>>({});

  /** Search params without the group key (shared by result and group pages). */
  const baseQuery = $derived.by(() => {
    const p = new URLSearchParams(params);
    p.delete('key');
    return p.toString();
  });

  $effect(() => {
    const qs = baseQuery;
    error = '';
    api
      .get<SearchResult>(`/search?${qs}`)
      .then((r) => {
        result = r;
        for (const f of [...r.facets.persons, ...r.facets.places, ...r.facets.events, ...r.facets.others]) tagNames[f.id] = f.name;
        for (const t of r.terms.flatMap((t) => t.tags)) tagNames[t.id] = t.name;
      })
      .catch((e) => (error = errorText(e instanceof ApiError ? e.code : 'internal')));
  });

  function update(change: (p: URLSearchParams) => void) {
    const p = new URLSearchParams(params);
    change(p);
    navigate(`/search?${p.toString()}`);
  }

  const addTag = (f: Facet) =>
    update((p) => {
      p.set('tags', [...new Set([...tagIds, f.id])].join(','));
      p.delete('key');
    });
  const removeTag = (id: number) =>
    update((p) => {
      const rest = tagIds.filter((t) => t !== id);
      if (rest.length) p.set('tags', rest.join(','));
      else p.delete('tags');
      p.delete('key');
    });
  const setYear = (y: string | null) =>
    update((p) => {
      if (y) p.set('year', y);
      else p.delete('year');
      p.delete('key');
    });
  const setGroup = (g: 'event' | 'year') =>
    update((p) => {
      p.set('group', g);
      p.delete('key');
    });
  const openGroup = (k: string | null) =>
    update((p) => {
      if (k) p.set('key', k);
      else p.delete('key');
    });

  const groupName = (g: { key: string; name: string | null }) => g.name ?? (group === 'year' ? s.noDate : s.noEvent);
  const current = $derived(key && result ? result.groups.find((g) => g.key === key) : null);

  const loadGroup = (offset: number) =>
    api.get<ClientPage>(`/search/media?${baseQuery}${key ? `&key=${encodeURIComponent(key)}` : ''}&offset=${offset}`);

  const facetSections = $derived(
    result
      ? ([
          [s.persons, result.facets.persons],
          [s.places, result.facets.places],
          [s.events, result.facets.events],
          [s.others, result.facets.others],
        ] as [string, Facet[]][]).filter(([, list]) => list.some((f) => !tagIds.includes(f.id)))
      : [],
  );
</script>

<div class="search-page">
  <aside class="facets">
    {#if result && result.total > 0}
      <h3>{s.groupBy}</h3>
      <div class="seg">
        <button class:on={group === 'event'} onclick={() => setGroup('event')}>{s.byEvent}</button>
        <button class:on={group === 'year'} onclick={() => setGroup('year')}>{s.byYear}</button>
      </div>
      {#each facetSections as [title, list] (title)}
        <h3>{title}</h3>
        {#each list.filter((f) => !tagIds.includes(f.id)) as f (f.id)}
          <button class="facet" onclick={() => addTag(f)}><span>{f.name}</span><span class="muted">{f.count}</span></button>
        {/each}
      {/each}
      {#if result.facets.years.length > 1 && !params.get('year')}
        <h3>{s.years}</h3>
        <div class="year-list">
          {#each result.facets.years as [y, n] (y)}
            <button class="facet" onclick={() => setYear(y)}><span>{y}</span><span class="muted">{n}</span></button>
          {/each}
        </div>
      {/if}
    {/if}
  </aside>

  <div class="results">
    {#if error}<div class="alert error">{error}</div>{/if}

    <div class="head">
      {#if result}
        <strong>{s.results(result.total)}</strong>
        {#each result.terms as term (term.word)}
          {#each term.tags.slice(0, 3) as t (t.id)}
            <span class="chip soft" title={s.matched}><span class="dot" style:background={t.color ?? 'var(--accent)'}></span>{t.name}</span>
          {/each}
        {/each}
      {/if}
      {#each tagIds as id (id)}
        <span class="chip">{tagNames[id] ?? `#${id}`}<button onclick={() => removeTag(id)} aria-label={s.clear}><IconX size={12} /></button></span>
      {/each}
      {#if params.get('year')}
        <span class="chip">{params.get('year')}<button onclick={() => setYear(null)} aria-label={s.clear}><IconX size={12} /></button></span>
      {/if}
    </div>

    {#if key}
      <div class="group-head">
        <button class="btn sm" onclick={() => openGroup(null)}><IconChevronLeft size={15} />{s.back}</button>
        {#if current}<h2>{groupName(current)}</h2><span class="muted">{current.count} {s.items}</span>{/if}
      </div>
      {#key router.search}<PhotoGrid load={loadGroup} />{/key}
    {:else if result}
      {#if result.total === 0}
        <div class="card empty muted">{s.none}</div>
      {:else}
        <div class="groups">
          {#each result.groups as g (g.key)}
            <AlbumCard
              title={groupName(g)}
              count={g.count}
              cover={g.cover_media_id}
              from={group === 'event' ? g.date_from : null}
              to={g.date_to}
              unit={s.items}
              onclick={() => openGroup(g.key)}
            />
          {/each}
        </div>
      {/if}
    {:else}
      <div class="muted">{tr.common.loading}</div>
    {/if}
  </div>
</div>

<style>
  .search-page {
    display: grid;
    grid-template-columns: 230px 1fr;
    gap: 20px;
    padding: 20px 24px;
    max-width: 1500px;
    margin: 0 auto;
    align-items: start;
  }
  .facets {
    display: grid;
    gap: 2px;
    position: sticky;
    top: 0;
  }
  h3 {
    margin: 12px 0 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .facets h3:first-child {
    margin-top: 0;
  }
  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .seg button {
    flex: 1;
    height: 30px;
    border: none;
    background: var(--surface);
    cursor: pointer;
    font: inherit;
    font-size: 13px;
  }
  .seg button.on {
    background: var(--accent);
    color: var(--accent-text);
  }
  .facet {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 5px 8px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    cursor: pointer;
    font: inherit;
    font-size: 13px;
    text-align: left;
  }
  .year-list {
    display: grid;
    gap: 2px;
  }
  .facet:hover {
    background: var(--accent-soft);
  }
  .results {
    display: grid;
    gap: 14px;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 4px 0 10px;
    border-radius: 999px;
    background: var(--accent-soft);
    font-size: 13px;
  }
  .chip.soft {
    padding-right: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
  }
  .chip button {
    border: none;
    background: transparent;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    cursor: pointer;
    color: var(--text-muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .groups {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 14px;
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .empty {
    padding: 28px;
    text-align: center;
  }
  @media (max-width: 760px) {
    .search-page {
      grid-template-columns: 1fr;
      padding: 12px;
    }
    .facets {
      position: static;
    }
  }
</style>
