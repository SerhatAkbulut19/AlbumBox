<script lang="ts">
  import { link, navigate, router } from '../lib/router.svelte';
  import { tr } from '../lib/i18n/tr';
  import { IconSearch } from '../lib/icons';
  import UserMenu from '../lib/UserMenu.svelte';
  import Home from './Home.svelte';
  import SearchPage from './SearchPage.svelte';
  import AlbumPage from './AlbumPage.svelte';

  const albumId = $derived(router.path.match(/^\/album\/(\d+)$/)?.[1]);
  const onSearch = $derived(router.path === '/search');

  // The top search box mirrors ?q= on the search page.
  let text = $state(new URLSearchParams(router.search).get('q') ?? '');
  $effect(() => {
    if (onSearch) text = new URLSearchParams(router.search).get('q') ?? '';
  });

  function submit(e: Event) {
    e.preventDefault();
    const q = text.trim();
    navigate(q ? `/search?q=${encodeURIComponent(q)}` : '/');
  }
</script>

<div class="shell">
  <header class="topbar">
    <a class="brand" href="/" use:link>
      <img src="/favicon.svg" alt="" width="24" height="24" />
      <span>{tr.app.name}</span>
    </a>
    <form class="search" onsubmit={submit}>
      <IconSearch size={17} class="search-icon" />
      <input
        class="input"
        type="search"
        bind:value={text}
        placeholder={tr.search.placeholder}
        enterkeyhint="search"
        onkeydown={(e) => e.key === 'Enter' && submit(e)}
      />
      <button type="submit" class="hidden-submit" tabindex="-1" aria-label={tr.search.button}></button>
    </form>
    <UserMenu area="client" />
  </header>
  <main class="content">
    {#if onSearch}
      <SearchPage />
    {:else if albumId}
      {#key albumId}<AlbumPage id={Number(albumId)} />{/key}
    {:else}
      <Home />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-rows: auto 1fr;
    height: 100%;
  }
  .topbar {
    height: 56px;
    padding: 0 20px;
    display: flex;
    align-items: center;
    gap: 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    color: var(--text);
  }
  .search {
    position: relative;
    flex: 1;
    max-width: 560px;
    margin: 0 auto;
  }
  .search :global(.search-icon) {
    position: absolute;
    left: 11px;
    top: 9px;
    color: var(--text-muted);
  }
  .hidden-submit {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    pointer-events: none;
  }
  .search .input {
    height: 36px;
    padding-left: 36px;
    border-radius: 999px;
    background: var(--surface-2);
  }
  .content {
    overflow: auto;
  }
  @media (max-width: 760px) {
    .brand span {
      display: none;
    }
    .topbar {
      padding: 0 10px;
      gap: 8px;
    }
  }
</style>
