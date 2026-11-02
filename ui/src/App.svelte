<script lang="ts">
  import { session } from './lib/session.svelte';
  import { router, navigate, isUnder } from './lib/router.svelte';
  import { tr } from './lib/i18n/tr';
  import Setup from './pages/Setup.svelte';
  import Login from './pages/Login.svelte';
  import AdminLayout from './admin/AdminLayout.svelte';
  import ClientLayout from './client/ClientLayout.svelte';

  session.init();

  const inAdmin = $derived(isUnder(router.path, '/admin'));

  // Members never stay inside the admin area.
  $effect(() => {
    if (session.user && inAdmin && !session.isAdmin) navigate('/', true);
  });
</script>

{#if !session.ready}
  <div class="center-screen muted">{tr.common.loading}</div>
{:else if session.needsSetup}
  <Setup />
{:else if !session.user}
  <Login />
{:else if inAdmin && session.isAdmin}
  <AdminLayout />
{:else}
  <ClientLayout />
{/if}
