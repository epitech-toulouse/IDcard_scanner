<script lang="ts">
  import { onMount } from "svelte";
  import { api, events } from "./lib/api";
  import { active, appUpdate, navigate, reader, route, toast } from "./lib/app.svelte";
  import Toasts from "./components/Toasts.svelte";
  import UpdateBanner from "./components/UpdateBanner.svelte";
  import ReaderIndicator from "./components/ReaderIndicator.svelte";
  import Icon, { type IconName } from "./components/Icon.svelte";
  import Accueil from "./pages/Accueil.svelte";
  import Emargement from "./pages/Emargement.svelte";
  import Creneaux from "./pages/Creneaux.svelte";
  import Etudiants from "./pages/Etudiants.svelte";
  import Groupes from "./pages/Groupes.svelte";
  import Exports from "./pages/Exports.svelte";
  import Reglages from "./pages/Reglages.svelte";
  import Cartes from "./pages/Cartes.svelte";
  import Affichage from "./pages/Affichage.svelte";

  type NavItem = { path: string; label: string; icon: IconName; also?: string[] };
  const NAV: NavItem[] = [
    { path: "accueil", label: "Aujourd'hui", icon: "home" },
    { path: "emargement", label: "Émargement", icon: "nfc" },
    { path: "creneaux", label: "Créneaux", icon: "calendar" },
    { path: "etudiants", label: "Étudiants & groupes", icon: "users", also: ["groupes"] },
    { path: "exports", label: "Exports", icon: "download" },
  ];
  const TOOLS: NavItem[] = [
    { path: "affichage", label: "Affichage salle", icon: "monitor" },
    { path: "cartes", label: "Test cartes", icon: "card" },
    { path: "reglages", label: "Réglages", icon: "settings" },
  ];
  const isOn = (n: NavItem) => page === n.path || !!n.also?.includes(page);

  let page = $derived(route.parts[0]);
  let param = $derived(route.parts[1] ? Number(route.parts[1]) : null);

  onMount(() => {
    api.readerStatus().then((s) => Object.assign(reader, s));
    api.getActiveSession().then((id) => (active.id = id));
    const offs = [
      events.onActiveSession((id) => (active.id = id)),
      events.onReader((s) => Object.assign(reader, s)),
      events.onError((m) => toast(m, "error", 7000)),
      events.onAppUpdate((u) => {
        if (appUpdate.info?.version !== u.version) appUpdate.dismissed = false;
        appUpdate.info = u;
      }),
      events.onAppUpdateProgress((p) => (appUpdate.progress = p)),
      events.onSessionAuto((e) => {
        if (e.action === "open") {
          api.getActiveSession().then((id) => (active.id = id));
          toast(`Émargement ouvert automatiquement : ${e.title}`, "success", 7000);
          // Écran d'émargement en attente d'un créneau : on y affiche celui qui vient d'ouvrir.
          if (route.parts[0] === "emargement" && !route.parts[1]) navigate(`emargement/${e.session_id}`);
        } else {
          if (active.id === e.session_id) api.getActiveSession().then((id) => (active.id = id));
          toast(`Créneau « ${e.title} » clos automatiquement · ${e.absents} absent(s)`, "info", 7000);
        }
      }),
    ];
    return () => offs.forEach((p) => p.then((off) => off()));
  });
</script>

<div class="shell">
  <nav aria-label="Navigation principale">
    <div class="brand">
      <span class="logo"><Icon name="nfc" size={18} stroke={2} /></span>
      <span>Badgeuse</span>
    </div>
    {#snippet item(n: NavItem)}
      <a href={"#/" + n.path} class:active={isOn(n)} aria-current={isOn(n) ? "page" : undefined}>
        <Icon name={n.icon} />
        <span class="grow">{n.label}</span>
        {#if n.path === "emargement" && active.id !== null}<span class="live">En cours</span>{/if}
      </a>
    {/snippet}
    {#each NAV as n (n.path)}{@render item(n)}{/each}
    <div class="section">Outils</div>
    {#each TOOLS as n (n.path)}{@render item(n)}{/each}
    <div class="spacer"></div>
    <ReaderIndicator />
  </nav>

  <main>
    <UpdateBanner />
    {#key page + (param ?? "")}
      {#if page === "accueil"}<Accueil />
      {:else if page === "emargement"}<Emargement sessionId={param} />
      {:else if page === "creneaux"}<Creneaux create={route.parts[1] === "nouveau"} />
      {:else if page === "etudiants"}<Etudiants />
      {:else if page === "groupes"}<Groupes />
      {:else if page === "exports"}<Exports />
      {:else if page === "affichage"}<Affichage />
      {:else if page === "cartes"}<Cartes />
      {:else if page === "reglages"}<Reglages />
      {:else}<p>Page inconnue.</p>
      {/if}
    {/key}
  </main>
</div>
<Toasts />

<style>
  .shell { display: grid; grid-template-columns: 248px 1fr; height: 100vh; }
  nav {
    background: var(--side);
    color: var(--side-text);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 1.2rem 0.9rem;
    gap: 0.25rem;
    overflow-y: auto;
  }
  .brand { display: flex; align-items: center; gap: 0.7rem; font-weight: 700; font-size: 1.15rem; color: var(--text); padding: 0.2rem 0.7rem 1.2rem; }
  .logo { width: 2.2rem; height: 2.2rem; border-radius: 9px; background: var(--accent); color: #fff; display: grid; place-items: center; }
  nav a {
    color: inherit;
    text-decoration: none;
    font-weight: 500;
    min-height: 2.9rem;
    padding: 0 0.8rem;
    border-radius: 8px;
    display: flex;
    gap: 0.8rem;
    align-items: center;
  }
  nav a:hover { background: var(--surface-2); color: var(--text); }
  nav a.active { background: var(--side-active); color: var(--side-active-text); font-weight: 600; }
  .live { font-size: 0.72rem; font-weight: 700; text-transform: uppercase; color: var(--present); background: var(--present-bg); padding: 0.1rem 0.5rem; border-radius: 99px; }
  .section { font-size: 0.75rem; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); padding: 1.3rem 0.8rem 0.4rem; }
  .spacer { flex: 1; min-height: 1.5rem; }
  main { overflow: auto; padding: 1.8rem 2.2rem 2.8rem; }
</style>
