<script lang="ts">
  import { onMount } from "svelte";
  import { api, events } from "./lib/api";
  import { reader, route, toast } from "./lib/app.svelte";
  import Toasts from "./components/Toasts.svelte";
  import ReaderIndicator from "./components/ReaderIndicator.svelte";
  import Accueil from "./pages/Accueil.svelte";
  import Emargement from "./pages/Emargement.svelte";
  import Creneaux from "./pages/Creneaux.svelte";
  import Etudiants from "./pages/Etudiants.svelte";
  import Groupes from "./pages/Groupes.svelte";
  import Exports from "./pages/Exports.svelte";
  import Reglages from "./pages/Reglages.svelte";

  const NAV = [
    { path: "accueil", label: "Accueil", icon: "⌂" },
    { path: "emargement", label: "Émargement", icon: "◉" },
    { path: "creneaux", label: "Créneaux", icon: "▦" },
    { path: "etudiants", label: "Étudiants", icon: "☺" },
    { path: "groupes", label: "Groupes", icon: "⋔" },
    { path: "exports", label: "Exports", icon: "⇩" },
    { path: "reglages", label: "Réglages", icon: "⚙" },
  ];

  let page = $derived(route.parts[0]);
  let param = $derived(route.parts[1] ? Number(route.parts[1]) : null);

  onMount(() => {
    api.readerStatus().then((s) => Object.assign(reader, s));
    const offs = [
      events.onReader((s) => Object.assign(reader, s)),
      events.onError((m) => toast(m, "error", 7000)),
    ];
    return () => offs.forEach((p) => p.then((off) => off()));
  });
</script>

<div class="shell">
  <nav>
    <div class="brand">
      <span class="logo">◎</span>
      <span>Badgeuse</span>
    </div>
    {#each NAV as n (n.path)}
      <a href={"#/" + n.path} class:active={page === n.path}>
        <span class="ico" aria-hidden="true">{n.icon}</span>{n.label}
      </a>
    {/each}
    <div class="spacer"></div>
    <ReaderIndicator />
  </nav>

  <main>
    {#key page + (param ?? "")}
      {#if page === "accueil"}<Accueil />
      {:else if page === "emargement"}<Emargement sessionId={param} />
      {:else if page === "creneaux"}<Creneaux />
      {:else if page === "etudiants"}<Etudiants />
      {:else if page === "groupes"}<Groupes />
      {:else if page === "exports"}<Exports />
      {:else if page === "reglages"}<Reglages />
      {:else}<p>Page inconnue.</p>
      {/if}
    {/key}
  </main>
</div>
<Toasts />

<style>
  .shell { display: grid; grid-template-columns: 210px 1fr; height: 100vh; }
  nav {
    background: var(--side);
    color: var(--side-text);
    display: flex;
    flex-direction: column;
    padding: 1rem 0.7rem;
    gap: 0.15rem;
  }
  .brand { display: flex; align-items: center; gap: 0.5rem; font-weight: 700; font-size: 1.15rem; color: #fff; padding: 0.2rem 0.6rem 1rem; }
  .logo { font-size: 1.4rem; color: #7fa1ff; }
  nav a {
    color: inherit;
    text-decoration: none;
    padding: 0.55rem 0.7rem;
    border-radius: 8px;
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }
  nav a:hover { background: rgb(255 255 255 / 7%); color: #fff; }
  nav a.active { background: var(--side-active); color: #fff; }
  .ico { width: 1.2rem; text-align: center; opacity: 0.9; }
  .spacer { flex: 1; }
  main { overflow: auto; padding: 1.5rem 1.8rem 2.5rem; }
</style>
