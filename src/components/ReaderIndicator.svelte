<script lang="ts">
  import { reader } from "../lib/app.svelte";

  const LABEL = { pcsc: "PC/SC", usb: "USB direct", mock: "Simulé", none: "" } as const;
</script>

<a class="reader" href="#/reglages" title={reader.message}>
  <span class="dot" class:on={reader.connected}></span>
  <span>
    <strong>{reader.connected ? "Lecteur prêt" : "Pas de lecteur"}</strong>
    <span class="sub">{reader.connected ? LABEL[reader.backend] : "Voir les réglages"}</span>
  </span>
</a>

<style>
  .reader {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.6rem 0.7rem;
    border-radius: 8px;
    background: rgb(255 255 255 / 6%);
    color: inherit;
    text-decoration: none;
    font-size: 0.85rem;
  }
  .reader strong { display: block; color: #fff; font-weight: 600; }
  .sub { opacity: 0.8; }
  .dot { width: 0.65rem; height: 0.65rem; border-radius: 50%; background: var(--absent); flex: none; box-shadow: 0 0 0 3px rgb(198 47 60 / 25%); }
  .dot.on { background: var(--present); box-shadow: 0 0 0 3px rgb(30 142 78 / 30%); }
</style>
