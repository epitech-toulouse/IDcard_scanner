<script lang="ts">
  import { appUpdate, installAppUpdate } from "../lib/app.svelte";

  const pct = $derived(
    appUpdate.progress?.total ? Math.min(100, Math.round((appUpdate.progress.downloaded / appUpdate.progress.total) * 100)) : null,
  );
</script>

{#if appUpdate.info && (!appUpdate.dismissed || appUpdate.installing)}
  <div class="banner" role="status">
    {#if appUpdate.installing}
      <span>
        Installation de Badgeuse {appUpdate.info.version}…
        {#if pct !== null}téléchargement {pct} %{:else if appUpdate.progress}vérification et installation{/if}
      </span>
    {:else}
      <span>
        <strong>Badgeuse {appUpdate.info.version}</strong> est disponible (installée : {appUpdate.info.current}).
      </span>
      <span class="actions">
        <button class="primary" onclick={installAppUpdate}>Installer et redémarrer</button>
        <button onclick={() => (appUpdate.dismissed = true)}>Plus tard</button>
      </span>
    {/if}
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 1rem;
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-left: 5px solid var(--accent);
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .actions { display: flex; gap: 0.5rem; flex-shrink: 0; }
</style>
