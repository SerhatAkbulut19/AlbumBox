<script lang="ts">
  import { session } from '../lib/session.svelte';
  import { ApiError } from '../lib/api';
  import { tr, errorText } from '../lib/i18n/tr';
  import AuthCard from './AuthCard.svelte';

  let username = $state('');
  let displayName = $state('');
  let password = $state('');
  let password2 = $state('');
  let error = $state('');
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    if (password !== password2) {
      error = errorText('password_mismatch');
      return;
    }
    busy = true;
    try {
      await session.setup(username, displayName, password);
    } catch (err) {
      error = errorText(err instanceof ApiError ? err.code : 'internal');
    } finally {
      busy = false;
    }
  }
</script>

<AuthCard title={tr.setup.title}>
  <p class="muted intro">{tr.setup.intro}</p>
  <form class="form" onsubmit={submit}>
    <label class="field">
      <span>{tr.login.username}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" bind:value={username} autocomplete="username" autofocus required />
    </label>
    <label class="field">
      <span>{tr.user.displayName}</span>
      <input class="input" bind:value={displayName} required />
    </label>
    <label class="field">
      <span>{tr.login.password}</span>
      <input class="input" type="password" bind:value={password} autocomplete="new-password" required />
    </label>
    <label class="field">
      <span>{tr.user.passwordRepeat}</span>
      <input class="input" type="password" bind:value={password2} autocomplete="new-password" required />
    </label>
    {#if error}<div class="alert error">{error}</div>{/if}
    <button class="btn primary" type="submit" disabled={busy}>{tr.setup.submit}</button>
  </form>
</AuthCard>

<style>
  .intro {
    margin: -6px 0 0;
    font-size: 13px;
  }
  .form {
    display: grid;
    gap: 12px;
  }
</style>
