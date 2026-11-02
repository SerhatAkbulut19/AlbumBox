<script lang="ts">
  import { tr } from '../lib/i18n/tr';
  import { IconHammer } from '../lib/icons';

  let { title }: { title: string } = $props();
</script>

<div class="page">
  <div class="card empty">
    <IconHammer size={28} stroke={1.5} class="ph-icon" />
    <h2>{title}</h2>
    <p class="muted">{tr.common.comingSoon}</p>
  </div>
</div>

<style>
  .empty {
    padding: 28px;
    text-align: center;
    display: grid;
    gap: 6px;
  }
  .empty :global(.ph-icon) {
    justify-self: center;
    color: var(--text-muted);
  }
  .empty p {
    margin: 0;
  }
</style>
