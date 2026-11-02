<script lang="ts">
  import type { Snippet } from 'svelte';
  import { tr } from '../lib/i18n/tr';

  let { title, children }: { title: string; children: Snippet } = $props();
</script>

<div class="center-screen">
  <div class="card auth">
    <div class="brand">
      <img src="/favicon.svg" alt="" width="34" height="34" />
      <div>
        <div class="name">{tr.app.name}</div>
        <div class="muted tagline">{tr.app.tagline}</div>
      </div>
    </div>
    <h1>{title}</h1>
    {@render children()}
  </div>
</div>

<style>
  .auth {
    width: 340px;
    padding: 24px;
    display: grid;
    gap: 16px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .name {
    font-weight: 600;
    font-size: 16px;
  }
  .tagline {
    font-size: 12px;
  }
</style>
