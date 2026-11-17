<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, onclose, wide = false, children, footer }: {
    title: string;
    onclose: () => void;
    wide?: boolean;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  function onkey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal" class:wide role="dialog" aria-modal="true" aria-label={title}>
    <header>
      <h2>{title}</h2>
      <button class="icon link" onclick={onclose} aria-label="Fermer">✕</button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgb(10 14 24 / 45%); display: grid; place-items: center; z-index: 50; padding: 1rem; }
  .modal {
    background: var(--surface);
    border-radius: 12px;
    width: min(560px, 100%);
    max-height: calc(100vh - 2rem);
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 50px rgb(0 0 0 / 30%);
  }
  .modal.wide { width: min(980px, 100%); }
  header { display: flex; justify-content: space-between; align-items: center; padding: 1rem 1.2rem 0.4rem; }
  header h2 { margin: 0; }
  .body { padding: 0.6rem 1.2rem 1rem; overflow: auto; display: flex; flex-direction: column; gap: 0.8rem; }
  footer { padding: 0.8rem 1.2rem; border-top: 1px solid var(--border); display: flex; justify-content: flex-end; gap: 0.5rem; }
</style>
