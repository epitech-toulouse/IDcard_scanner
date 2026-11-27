<script lang="ts">
  // « Aujourd'hui » : le créneau en cours, le programme du jour et ce qui demande une action.
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { active, guard, navigate } from "../lib/app.svelte";
  import { addDays, addMinutes, endOfDay, fmtDate, fmtTime, pct, startOfDay, toNaive } from "../lib/format";
  import Icon from "../components/Icon.svelte";
  import { STATE_LABEL, type ProgramNode, type SessionRow, type Settings } from "../lib/types";

  let today = $state<SessionRow[]>([]);
  let recent = $state<SessionRow[]>([]);
  let tree = $state<ProgramNode[]>([]);
  let students = $state(0);
  let withoutCard = $state(0);
  let current = $state<SessionRow | null>(null);
  let settings = $state<Settings | null>(null);
  let loaded = $state(false);

  let cohorts = $derived(tree.reduce((n, p) => n + p.cohorts.length, 0));
  let groups = $derived(tree.reduce((n, p) => n + p.cohorts.reduce((m, c) => m + c.groups.length, 0), 0));
  const nowNaive = toNaive(new Date());
  /** Créneaux terminés mais jamais clos (les absents ne sont pas encore comptés). */
  let unclosed = $derived(recent.filter((s) => s.state !== "closed" && s.ends_at < nowNaive && s.id !== current?.id));
  let rate = $derived.by(() => {
    const done = recent.filter((s) => s.state === "closed");
    const here = done.reduce((n, s) => n + s.counts.present + s.counts.late, 0);
    return pct(here, done.reduce((n, s) => n + s.counts.expected, 0));
  });
  let todoCount = $derived((withoutCard ? 1 : 0) + unclosed.length);

  onMount(async () => {
    const now = new Date();
    const [t, r, tr, st, set] = await Promise.all([
      guard(api.listSessions(startOfDay(now), endOfDay(now))),
      guard(api.listSessions(addDays(startOfDay(now), -30), endOfDay(now))),
      guard(api.tree()),
      guard(api.listStudents({})),
      guard(api.getSettings()),
    ]);
    today = (t ?? []).reverse();
    recent = r ?? [];
    tree = tr ?? [];
    students = st?.length ?? 0;
    withoutCard = st?.filter((s) => s.cards.length === 0).length ?? 0;
    settings = set ?? null;
    const id = await api.getActiveSession();
    active.id = id;
    if (id !== null) current = (await guard(api.sessionDetail(id)))?.session ?? null;
    loaded = true;
  });

  const greeting = new Date().getHours() < 18 ? "Bonjour" : "Bonsoir";
  const dateLabel = new Date().toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long" });

  const here = (s: SessionRow) => s.counts.present + s.counts.late;
  const share = (n: number, s: SessionRow) => (s.counts.expected ? Math.min(100, (100 * n) / s.counts.expected) : 0);
  function autoHint(s: SessionRow): string {
    if (s.state !== "planned" || !settings?.auto_open) return "";
    return `auto ${fmtTime(addMinutes(s.starts_at, -settings.auto_open_lead_min))}`;
  }
</script>

<div class="page-head">
  <div>
    <h1>{greeting}</h1>
    <p class="muted"><span class="cap">{dateLabel}</span> · {today.length} créneau{today.length > 1 ? "x" : ""} aujourd'hui</p>
  </div>
  <button class="primary" onclick={() => navigate("creneaux/nouveau")}><Icon name="plus" size={16} stroke={2.2} />Nouveau créneau</button>
</div>

{#if current}
  {@const s = current}
  <section class="card hero" aria-label="Créneau en cours">
    <div class="hero-info">
      <span class="kicker live"><span class="pulse"></span>Émargement {s.state === "open" ? "ouvert" : STATE_LABEL[s.state].toLowerCase()}</span>
      <h2>{s.title}</h2>
      <p class="muted">{fmtTime(s.starts_at)} – {fmtTime(s.ends_at)}{s.room ? ` · ${s.room}` : ""} · {s.targets}</p>
    </div>
    <div class="hero-count">
      <div class="count-line">
        <span class="num">{here(s)}</span><span class="muted">/ {s.counts.expected} présents</span>
        <span class="grow"></span>
        <span class="small muted">{Math.max(0, s.counts.expected - here(s) - s.counts.excused - s.counts.absent)} non badgés</span>
      </div>
      <div class="bar" role="img" aria-label="{here(s)} sur {s.counts.expected} présents, dont {s.counts.late} en retard">
        <span class="ok" style:width="{share(s.counts.present, s)}%"></span>
        <span class="lt" style:width="{share(s.counts.late, s)}%"></span>
      </div>
      <div class="legend small muted">
        <span><i class="sw ok"></i>{s.counts.present} à l'heure</span>
        <span><i class="sw lt"></i>{s.counts.late} en retard</span>
      </div>
    </div>
    <div class="row">
      <button class="primary big" onclick={() => navigate(`emargement/${s.id}`)}>Reprendre l'émargement</button>
      <button class="big" onclick={() => navigate("affichage")}><Icon name="monitor" size={16} />Projeter</button>
    </div>
  </section>
{/if}

<div class="cols">
  <section class="card flush day" aria-labelledby="h-jour">
    <div class="card-head">
      <h2 id="h-jour">Programme du jour</h2>
      <a href="#/creneaux" class="small">Voir la semaine</a>
    </div>
    {#each today as s (s.id)}
      <div class="session">
        <div class="time"><strong>{fmtTime(s.starts_at)}</strong><span class="small muted">→ {fmtTime(s.ends_at)}</span></div>
        <div class="grow info">
          <strong>{s.title}</strong>
          <span class="small muted">{s.room ? `${s.room} · ` : ""}{s.targets}</span>
        </div>
        <span class="small muted ratio">{s.state === "planned" ? "—" : here(s)}/{s.counts.expected}</span>
        <span class="pill {s.state}">{STATE_LABEL[s.state]}</span>
        <span class="act">
          {#if s.state === "planned" && autoHint(s)}
            <span class="small muted">{autoHint(s)}</span>
          {:else}
            <button class="link" onclick={() => navigate(`emargement/${s.id}`)}>{s.state === "closed" ? "Voir" : "Émarger"}</button>
          {/if}
        </span>
      </div>
    {:else}
      <p class="empty">Aucun créneau prévu aujourd'hui.</p>
    {/each}
  </section>

  <section class="card flush todo" aria-labelledby="h-todo">
    <div class="card-head">
      <h2 id="h-todo">À traiter</h2>
      {#if todoCount}<span class="pill warn">{todoCount}</span>{/if}
    </div>
    {#if withoutCard}
      <div class="task">
        <span class="ico late"><Icon name="card" /></span>
        <span class="grow"><strong>{withoutCard} étudiant{withoutCard > 1 ? "s" : ""} sans carte</strong><span class="small muted">À pointer à la main, ou associer au premier badge</span></span>
        <a href="#/etudiants" class="small strong-link">Voir</a>
      </div>
    {/if}
    {#each unclosed as s (s.id)}
      <div class="task">
        <span class="ico absent"><Icon name="calendar" /></span>
        <span class="grow"><strong>Créneau non clos</strong><span class="small muted">{s.title} · {fmtDate(s.starts_at)}</span></span>
        <a href={`#/emargement/${s.id}`} class="small strong-link">Clore</a>
      </div>
    {/each}
    {#if loaded && !todoCount}
      <p class="empty small">Rien à traiter.</p>
    {/if}
  </section>
</div>

<div class="tiles">
  <a class="card tile" href="#/etudiants"><span class="small muted">Étudiants actifs</span><span class="value">{students}</span></a>
  <a class="card tile" href="#/groupes"><span class="small muted">Promotions · groupes</span><span class="value">{cohorts} · {groups}</span></a>
  <a class="card tile" href="#/exports"><span class="small muted">Présence sur 30 jours</span><span class="value">{rate}</span></a>
  <a class="card tile" href="#/creneaux"><span class="small muted">Créneaux sur 30 jours</span><span class="value">{recent.length}</span></a>
</div>

{#if loaded && students === 0}
  <section class="card start">
    <h2>Pour commencer</h2>
    <ol>
      <li>Branchez le lecteur NFC (voir <a href="#/reglages">Réglages</a> s'il n'est pas détecté).</li>
      <li><a href="#/etudiants">Importez la liste des étudiants</a> (CSV ou Excel de la scolarité) : cursus, promotion et groupes sont créés.</li>
      <li><a href="#/creneaux">Créez un créneau</a> pour un ou plusieurs groupes, puis ouvrez l'émargement.</li>
      <li>Au premier badge, une carte inconnue vous demande à quel étudiant l'associer.</li>
      <li>Clôturez : les non-badgés passent absents. Exportez en Excel.</li>
    </ol>
  </section>
{/if}

<style>
  .cap { text-transform: capitalize; }
  .hero { display: flex; flex-wrap: wrap; align-items: center; gap: 1.8rem; margin-bottom: 1.4rem; padding: 1.5rem 1.6rem; }
  .hero-info { flex: 1 1 320px; display: flex; flex-direction: column; gap: 0.35rem; }
  .hero-info h2 { margin: 0; font-size: 1.5rem; font-weight: 700; }
  .hero-info p { margin: 0; }
  .live { display: flex; align-items: center; gap: 0.5rem; color: var(--present); }
  .pulse { width: 0.55rem; height: 0.55rem; border-radius: 50%; background: var(--present-bar); box-shadow: 0 0 0 4px var(--present-bg); }
  .hero-count { flex: 1 1 260px; display: flex; flex-direction: column; gap: 0.6rem; }
  .count-line { display: flex; align-items: baseline; gap: 0.5rem; }
  .num { font-size: 2.5rem; font-weight: 800; letter-spacing: -0.02em; line-height: 1; }
  .bar { height: 10px; border-radius: 99px; background: var(--none-bg); display: flex; overflow: hidden; }
  .bar span { transition: width 0.3s; }
  .ok { background: var(--present-bar); }
  .lt { background: var(--late-bar); }
  .legend { display: flex; gap: 1rem; }
  .sw { display: inline-block; width: 0.55rem; height: 0.55rem; border-radius: 2px; margin-right: 0.4rem; }

  .cols { display: flex; flex-wrap: wrap; gap: 1.4rem; align-items: flex-start; margin-bottom: 1.4rem; }
  .day { flex: 999 1 520px; min-width: 0; }
  .todo { flex: 1 1 300px; min-width: 0; }
  .session { display: flex; align-items: center; gap: 1.1rem; padding: 0.95rem 1.4rem; border-bottom: 1px solid var(--border); }
  .session:last-child { border-bottom: none; }
  .session:hover { background: var(--surface-3); }
  .time { width: 5.5rem; flex: none; display: flex; flex-direction: column; }
  .info { display: flex; flex-direction: column; min-width: 0; }
  .ratio { width: 3.5rem; text-align: right; }
  .session .pill { width: 5.5rem; text-align: center; }
  .act { width: 6rem; text-align: right; }

  .task { display: flex; align-items: center; gap: 0.9rem; padding: 0.95rem 1.4rem; border-bottom: 1px solid var(--border); }
  .task:last-child { border-bottom: none; }
  .task .grow { display: flex; flex-direction: column; min-width: 0; }
  .ico { width: 2.3rem; height: 2.3rem; border-radius: 9px; display: grid; place-items: center; flex: none; }
  .ico.late { background: var(--late-bg); color: var(--late); }
  .ico.absent { background: var(--absent-bg); color: var(--absent); }
  .strong-link { font-weight: 600; text-decoration: none; }

  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 1rem; }
  .tile { display: flex; flex-direction: column; gap: 0.1rem; text-decoration: none; color: inherit; padding: 1rem 1.2rem; border-radius: 12px; }
  .tile:hover { border-color: var(--accent); color: inherit; }
  .value { font-size: 1.6rem; font-weight: 700; }
  .start { margin-top: 1.4rem; }
  ol { margin: 0; padding-left: 1.3rem; line-height: 1.9; }
</style>
