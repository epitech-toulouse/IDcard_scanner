<script lang="ts">
  // Encadré « Lecteurs » en bas de la barre latérale : un point par lecteur (local ou borne).
  import { reader } from "../lib/app.svelte";

  const LABEL = { pcsc: "PC/SC", usb: "USB direct", mock: "Simulé", remote: "Borne", none: "" } as const;

  const display = (name: string) => (name.startsWith("borne:") ? `Borne ${name.slice(6)}` : name);
</script>

<a class="readers" href="#/reglages" title={reader.message}>
  <span class="head">Lecteurs</span>
  {#each reader.readers as r (r.name)}
    <span class="line">
      <span class="dot" class:on={r.connected}></span>
      <span class="name" title={r.name}>{display(r.name)}</span>
      <span class="state" class:on={r.connected}>{r.connected ? (r.backend === "remote" ? "En ligne" : "Prêt") : "Absent"}</span>
    </span>
  {:else}
    <span class="line">
      <span class="dot" class:on={reader.connected}></span>
      <span class="name">{reader.connected ? LABEL[reader.backend] || "Lecteur" : "Aucun lecteur"}</span>
      <span class="state" class:on={reader.connected}>{reader.connected ? "Prêt" : "Réglages"}</span>
    </span>
  {/each}
</a>

<style>
  .readers {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 0.8rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    color: var(--text);
    text-decoration: none;
  }
  .readers:hover { background: var(--surface-3); color: var(--text); }
  .head { font-size: 0.75rem; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); }
  .line { display: flex; align-items: center; gap: 0.6rem; font-size: 0.93rem; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .state { font-size: 0.82rem; font-weight: 600; color: var(--absent); }
  .state.on { color: var(--present); }
  .dot { width: 0.55rem; height: 0.55rem; border-radius: 50%; background: var(--absent); flex: none; }
  .dot.on { background: var(--present-bar); }
</style>
