<script lang="ts">
  // Shared top-right controls: area switch (admin only), theme, user, logout.
  import { session } from './session.svelte';
  import { theme } from './theme.svelte';
  import { link } from './router.svelte';
  import { tr } from './i18n/tr';
  import { IconLibraryPhoto, IconLogout, IconMoon, IconSettings, IconSun } from './icons';

  let { area }: { area: 'admin' | 'client' } = $props();
</script>

<div class="menu">
  {#if session.isAdmin}
    {#if area === 'admin'}
      <a class="btn sm ghost" href="/" use:link><IconLibraryPhoto size={16} />{tr.nav.client}</a>
    {:else}
      <a class="btn sm ghost" href="/admin" use:link><IconSettings size={16} />{tr.nav.admin}</a>
    {/if}
  {/if}
  <button class="btn sm ghost" onclick={() => theme.toggle()} title={tr.nav.theme} aria-label={tr.nav.theme}>
    {#if theme.current === 'dark'}<IconSun size={16} />{:else}<IconMoon size={16} />{/if}
  </button>
  <span class="who">
    {session.user?.display_name}
    <span class="badge {session.isAdmin ? 'accent' : ''}">{tr.user.roles[session.user?.role ?? 'member']}</span>
  </span>
  <button class="btn sm" onclick={() => session.logout()}><IconLogout size={16} />{tr.nav.logout}</button>
</div>

<style>
  .menu {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .who {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    padding: 0 4px;
  }
</style>
