<script lang="ts">
  import { router, link, isUnder } from '../lib/router.svelte';
  import { tr } from '../lib/i18n/tr';
  import UserMenu from '../lib/UserMenu.svelte';
  import Dashboard from './Dashboard.svelte';
  import Users from './Users.svelte';
  import Libraries from './Libraries.svelte';
  import Categories from './Categories.svelte';
  import Tags from './Tags.svelte';
  import Profiles from './Profiles.svelte';
  import Persons from './Persons.svelte';
  import MediaPage from './media/MediaPage.svelte';
  import Placeholder from './Placeholder.svelte';
  import { engine } from '../lib/engine.svelte';
  import {
    IconCategory,
    IconDatabase,
    IconFaceId,
    IconLayoutDashboard,
    IconPhoto,
    IconTags,
    IconUsers,
    IconUsersGroup,
  } from '../lib/icons';

  // Live engine status for the whole admin area.
  $effect(() => {
    engine.connect();
    return () => engine.disconnect();
  });

  const items = [
    { href: '/admin', label: tr.nav.dashboard, icon: IconLayoutDashboard },
    { href: '/admin/media', label: tr.nav.media, icon: IconPhoto },
    { href: '/admin/categories', label: tr.nav.categories, icon: IconCategory },
    { href: '/admin/tags', label: tr.nav.tags, icon: IconTags },
    { href: '/admin/persons', label: tr.nav.persons, icon: IconFaceId },
    { href: '/admin/users', label: tr.nav.users, icon: IconUsers },
    { href: '/admin/profiles', label: tr.nav.profiles, icon: IconUsersGroup },
    { href: '/admin/libraries', label: tr.nav.libraries, icon: IconDatabase },
  ];

  const current = $derived(
    items.find((i) => (i.href === '/admin' ? router.path === '/admin' : isUnder(router.path, i.href))) ?? items[0],
  );
</script>

<div class="shell">
  <aside class="sidebar">
    <div class="brand">
      <img src="/favicon.svg" alt="" width="24" height="24" />
      <span>{tr.app.name}</span>
      <span class="badge accent">{tr.nav.admin}</span>
    </div>
    <nav>
      {#each items as item}
        <a href={item.href} use:link class:active={item === current}>
          <item.icon size={18} stroke={1.75} class="nav-icon" />{item.label}
          {#if item.href === '/admin/libraries' && engine.running}<span class="pulse" title={tr.engine.scanning}></span>{/if}
        </a>
      {/each}
    </nav>
  </aside>
  <div class="main">
    <header class="topbar">
      <h1>{current.label}</h1>
      <UserMenu area="admin" />
    </header>
    <div class="content" class:fixed={current.href === '/admin/media'}>
      {#if current.href === '/admin'}
        <Dashboard />
      {:else if current.href === '/admin/media'}
        <MediaPage />
      {:else if current.href === '/admin/categories'}
        <Categories />
      {:else if current.href === '/admin/tags'}
        <Tags />
      {:else if current.href === '/admin/persons'}
        <Persons />
      {:else if current.href === '/admin/profiles'}
        <Profiles />
      {:else if current.href === '/admin/users'}
        <Users />
      {:else if current.href === '/admin/libraries'}
        <Libraries />
      {:else}
        <Placeholder title={current.label} />
      {/if}
    </div>
  </div>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar-w) 1fr;
    height: 100%;
  }
  .sidebar {
    background: var(--surface);
    border-right: 1px solid var(--border);
    padding: 14px 10px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    padding: 0 6px;
  }
  nav {
    display: grid;
    gap: 2px;
  }
  nav a {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    color: var(--text);
  }
  nav a:hover {
    background: var(--surface-2);
  }
  nav a.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  nav :global(.nav-icon) {
    flex: none;
    opacity: 0.75;
  }
  nav a.active :global(.nav-icon) {
    opacity: 1;
  }
  .pulse {
    margin-left: auto;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
  .main {
    display: grid;
    grid-template-rows: auto 1fr;
    min-width: 0;
    min-height: 0;
  }
  .topbar {
    height: 52px;
    padding: 0 24px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .content {
    overflow: auto;
    min-height: 0;
  }
  .content.fixed {
    overflow: hidden;
  }
</style>
