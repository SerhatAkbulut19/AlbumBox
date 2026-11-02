<script lang="ts">
  import { session } from '../lib/session.svelte';
  import { ApiError } from '../lib/api';
  import { tr, errorText } from '../lib/i18n/tr';
  import AuthCard from './AuthCard.svelte';

  let username = $state('');
  let password = $state('');
  let remember = $state(true);
  let error = $state('');
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    busy = true;
    try {
      await session.login(username, password, remember);
    } catch (err) {
      error = errorText(err instanceof ApiError ? err.code : 'internal');
      password = '';
    } finally {
      busy = false;
    }
  }
</script>

<AuthCard title={tr.login.title}>
  <form class="form" onsubmit={submit}>
    <label class="field">
      <span>{tr.login.username}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" bind:value={username} autocomplete="username" autofocus required />
    </label>
    <label class="field">
      <span>{tr.login.password}</span>
      <input class="input" type="password" bind:value={password} autocomplete="current-password" required />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={remember} />
      {tr.login.remember}
    </label>
    {#if error}<div class="alert error">{error}</div>{/if}
    <button class="btn primary" type="submit" disabled={busy}>{tr.login.submit}</button>
  </form>
</AuthCard>

<style>
  .form {
    display: grid;
    gap: 12px;
  }
</style>
