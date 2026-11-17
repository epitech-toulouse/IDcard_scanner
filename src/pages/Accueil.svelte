<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { guard, navigate, reader } from "../lib/app.svelte";
  import { endOfDay, fmtTime, startOfDay } from "../lib/format";
  import { STATE_LABEL, type ProgramNode, type SessionRow } from "../lib/types";

  let today = $state<SessionRow[]>([]);
  let tree = $state<ProgramNode[]>([]);
  let students = $state(0);
  let withoutCard = $state(0);
  let active = $state<number | null>(null);

  let cohorts = $derived(tree.reduce((n, p) => n + p.cohorts.length, 0));
  let groups = $derived(tree.reduce((n, p) => n + p.cohorts.reduce((m, c) => m + c.groups.length, 0), 0));

  onMount(async () => {
    const now = new Date();
    const [t, tr, st, a] = await Promise.all([
      guard(api.listSessions(startOfDay(now), endOfDay(now))),
      guard(api.tree()),
      guard(api.listStudents({})),
      api.getActiveSession(),
    ]);
    today = (t ?? []).reverse();
    tree = tr ?? [];
    students = st?.length ?? 0;
    withoutCard = st?.filter((s) => s.cards.length === 0).length ?? 0;
    active = a;
  });

  const greeting = new Date().getHours() < 18 ? "Bonjour" : "Bonsoir";
  const dateLabel = new Date().toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long" });
</script>

<div class="page-head">
  <div>
    <h1>{greeting} 👋</h1>
    <p class="muted" style:text-transform="capitalize">{dateLabel}</p>
  </div>
  <div class="row">
    {#if active !== null}<button class="primary big" onclick={() => navigate(`emargement/${active}`)}>Reprendre l'émargement en cours</button>{/if}
    <button class:primary={active === null} class:big={active === null} onclick={() => navigate("creneaux")}>Nouveau créneau</button>
  </div>
</div>

<div class="tiles">
  <a class="card tile" href="#/reglages">
    <span class="label">Lecteur NFC</span>
    <span class="value" class:ok={reader.connected} class:ko={!reader.connected}>{reader.connected ? "Connecté" : "Absent"}</span>
    <span class="small muted">{reader.message}</span>
  </a>
  <a class="card tile" href="#/etudiants">
    <span class="label">Étudiants</span>
    <span class="value">{students}</span>
    <span class="small muted">{withoutCard ? `${withoutCard} sans carte associée` : "tous ont une carte"}</span>
  </a>
  <a class="card tile" href="#/groupes">
    <span class="label">Promotions / groupes</span>
    <span class="value">{cohorts} / {groups}</span>
    <span class="small muted">{tree.length} cursus</span>
  </a>
</div>

<section class="card">
  <h2>Créneaux du jour</h2>
  {#each today as s (s.id)}
    <div class="session">
      <span class="time">{fmtTime(s.starts_at)} – {fmtTime(s.ends_at)}</span>
      <span class="grow"><strong>{s.title}</strong>{s.room ? ` · ${s.room}` : ""}<br /><span class="small muted">{s.targets}</span></span>
      <span class="small">{s.counts.present + s.counts.late}/{s.counts.expected}</span>
      <span class="pill {s.state}">{STATE_LABEL[s.state]}</span>
      <button class:primary={s.state !== "closed"} onclick={() => navigate(`emargement/${s.id}`)}>{s.state === "closed" ? "Voir" : "Émarger"}</button>
    </div>
  {:else}
    <p class="muted">Aucun créneau prévu aujourd'hui.</p>
  {/each}
</section>

{#if students === 0}
  <section class="card start">
    <h2>Pour commencer</h2>
    <ol>
      <li>Branchez le lecteur NFC (voir <a href="#/reglages">Réglages</a> si il n'est pas détecté).</li>
      <li><a href="#/etudiants">Importez la liste des étudiants</a> (CSV ou Excel de la scolarité) : cursus, promotion et groupes sont créés.</li>
      <li><a href="#/creneaux">Créez un créneau</a> pour un ou plusieurs groupes, puis ouvrez l'émargement.</li>
      <li>Au premier badge, une carte inconnue vous demande à quel étudiant l'associer.</li>
      <li>Clôturez : les non-badgés passent absents. Exportez en Excel.</li>
    </ol>
  </section>
{/if}

<style>
  .tiles { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; margin-bottom: 1rem; }
  .tile { display: flex; flex-direction: column; gap: 0.2rem; text-decoration: none; color: inherit; }
  .tile:hover { border-color: var(--accent); }
  .label { color: var(--muted); font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .value { font-size: 1.8rem; font-weight: 700; }
  .ok { color: var(--present); }
  .ko { color: var(--absent); }
  .session { display: flex; align-items: center; gap: 1rem; padding: 0.6rem 0; border-bottom: 1px solid var(--border); }
  .session:last-child { border-bottom: none; }
  .time { font-variant-numeric: tabular-nums; width: 7.5rem; }
  .start { margin-top: 1rem; }
  ol { margin: 0; padding-left: 1.3rem; line-height: 1.9; }
</style>
