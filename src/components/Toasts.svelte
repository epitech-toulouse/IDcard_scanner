<script lang="ts">
  import { dismiss, toasts } from "../lib/app.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each toasts as t (t.id)}
    <button class="toast {t.kind}" onclick={() => dismiss(t.id)}>{t.text}</button>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 1rem; bottom: 1rem; display: flex; flex-direction: column; gap: 0.5rem; z-index: 100; max-width: 420px; }
  .toast {
    text-align: left;
    white-space: normal;
    padding: 0.7rem 0.95rem;
    border-radius: 9px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 18%);
    border: 1px solid var(--border);
    border-left: 5px solid var(--accent);
    background: var(--surface);
  }
  .success { border-left-color: var(--present); }
  .warn { border-left-color: var(--late); }
  .error { border-left-color: var(--absent); }
</style>
