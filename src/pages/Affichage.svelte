<script lang="ts">
  // Affichage automatique : écran de projection plein écran qui suit tout seul le créneau actif.
  // Un créneau ouvert (à la main ou automatiquement) est projeté jusqu'à début + tolérance de retard ;
  // ensuite l'écran d'attente (horloge + « prochain émargement à … ») reprend, même si le créneau
  // reste ouvert pour les retardataires. Aucune action d'enseignant ici.
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, events } from "../lib/api";
  import { navigate, reader } from "../lib/app.svelte";
  import { endOfDay, fmtTime, parseNaive, startOfDay, toNaive } from "../lib/format";
  import { isProjected, nextOpeningAt, nextSession } from "../lib/display";
  import { describeScan, type ScanMessage } from "../lib/scan";
  import Projection from "../components/Projection.svelte";
  import type { ScanOutcome, SessionDetail, SessionRow, Settings } from "../lib/types";

  const POLL_MS = 10_000;

  /** Créneau actif non clos (projeté ou non selon l'heure). */
  let active = $state<SessionDetail | null>(null);
  let todayList = $state<SessionRow[]>([]);
  let settings = $state<Settings | null>(null);
  let flash = $state<ScanMessage | null>(null);
  let recent = $state<{ id: number; cls: string; title: string }[]>([]);
  let now = $state(new Date());
  let recentId = 0;
  let flashTimer: ReturnType<typeof setTimeout>;

  let nowN = $derived(toNaive(now));
  let detail = $derived(active && isProjected(active.session, nowN) ? active : null);
  let s = $derived(detail?.session ?? null);
  /** Créneau actif encore ouvert après sa fenêtre de projection : les retardataires peuvent badger. */
  let lateOpen = $derived(active && !detail && active.session.state === "open" ? active.session : null);
  let next = $derived(s ? null : nextSession(todayList, nowN, lateOpen?.id ?? null));
  let here = $derived(detail?.rows.filter((r) => r.status === "present" || r.status === "late").length ?? 0);
  let clock = $derived(now.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" }));
  let today = $derived(now.toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long" }));
  /** Heure d'ouverture du prochain émargement (automatique si les réglages l'activent, sinon le début). */
  let nextAt = $derived(next ? nextOpeningAt(next, settings?.auto_open ? settings.auto_open_lead_min : null) : null);

  async function refresh() {
    const id = await api.getActiveSession().catch(() => null);
    const d = id !== null ? await api.sessionDetail(id).catch(() => null) : null;
    const keep = d && d.session.state !== "closed" ? d : null;
    if (keep?.session.id !== active?.session.id) recent = [];
    active = keep;
    const day = new Date();
    todayList = await api.listSessions(startOfDay(day), endOfDay(day)).catch(() => [] as SessionRow[]);
  }

  function show(m: ScanMessage) {
    flash = m;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = null), 4000);
  }

  async function onCard(o: ScanOutcome) {
    const m = describeScan(o);
    // Pas de dialogue ici : l'étudiant est renvoyé vers l'enseignant.
    if (o.kind === "unknown_card" || o.kind === "not_expected") m.sub = `${o.kind === "unknown_card" ? "Carte non associée" : "Non attendu sur ce créneau"} · voir l'enseignant`;
    show(m);
    if (o.kind === "recorded") recent = [{ id: recentId++, cls: o.status, title: m.title }, ...recent].slice(0, 8);
    await refresh();
  }

  function exit() {
    getCurrentWindow().setFullscreen(false).catch(() => {});
    navigate(active ? `emargement/${active.session.id}` : "accueil");
  }

  onMount(() => {
    getCurrentWindow().setFullscreen(true).catch(() => {});
    api.getSettings().then((v) => (settings = v)).catch(() => {});
    refresh();
    const poll = setInterval(refresh, POLL_MS);
    const tick = setInterval(() => (now = new Date()), 1000);
    const offs = [events.onCard(onCard), events.onSessionAuto(() => refresh()), events.onActiveSession(() => refresh())];
    return () => {
      clearInterval(poll);
      clearInterval(tick);
      clearTimeout(flashTimer);
      offs.forEach((p) => p.then((f) => f()));
      getCurrentWindow().setFullscreen(false).catch(() => {});
    };
  });
</script>

{#snippet status()}
  <span class="tag">● Affichage automatique · {clock}</span>
  {#if !reader.connected}<span class="tag warn">⚠ Lecteur non connecté</span>{/if}
{/snippet}

<svelte:window onkeydown={(e) => e.key === "Escape" && !s && exit()} />

{#if s}
  <Projection session={s} {here} {flash} {recent} onexit={exit} exitLabel="Quitter l'affichage" badge={status} />
{:else}
  <div class="waiting">
    <div class="top">
      {@render status()}
      <button onclick={exit}>Quitter l'affichage</button>
    </div>
    <div class="clock">{clock}</div>
    <div class="date">{today}</div>
    {#if flash}
      <div class="pbanner {flash.cls}"><div class="pwho">{flash.title}</div><div class="psub">{flash.sub}</div></div>
    {:else if next && nextAt}
      <div class="next">
        <div class="muted">
          {next.state === "open" || parseNaive(nextAt) <= now ? "Émargement imminent" : `Prochain émargement à ${fmtTime(nextAt)}`}
        </div>
        <div class="title">{next.title}</div>
        <div>{fmtTime(next.starts_at)} – {fmtTime(next.ends_at)}{next.room ? ` · Salle ${next.room}` : ""}</div>
        <div class="muted small">{next.targets}</div>
        {#if !settings?.auto_open && next.state !== "open"}
          <div class="hint">Ouverture par l'enseignant</div>
        {/if}
      </div>
    {:else}
      <div class="next"><div class="title muted">Aucun autre créneau aujourd'hui</div></div>
    {/if}
    {#if lateOpen && !flash}
      <div class="hint">Retardataires : vous pouvez encore badger pour « {lateOpen.title} »</div>
    {/if}
  </div>
{/if}

<style>
  .waiting { position: fixed; inset: 0; z-index: 40; background: var(--bg); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 1.2rem; text-align: center; padding: 2rem; }
  .top { position: absolute; top: 1rem; right: 1rem; left: 1rem; display: flex; justify-content: flex-end; align-items: center; gap: 0.8rem; }
  .tag { margin-right: auto; color: var(--muted); font-size: 0.95rem; }
  .tag.warn { margin-right: 0; color: var(--absent); font-weight: 600; }
  .clock { font-size: 7rem; font-weight: 800; font-variant-numeric: tabular-nums; line-height: 1; }
  .date { font-size: 1.6rem; color: var(--muted); text-transform: capitalize; }
  .next { margin-top: 2rem; display: flex; flex-direction: column; gap: 0.4rem; font-size: 1.4rem; }
  .next .title { font-size: 2.4rem; font-weight: 700; }
  .hint { margin-top: 0.8rem; font-size: 1.2rem; color: var(--muted); }
  .pbanner { margin-top: 2rem; width: min(1000px, 92vw); min-height: 200px; border-radius: 20px; display: flex; flex-direction: column; justify-content: center; border: 3px solid var(--border); background: var(--surface); }
  .pbanner.present { border-color: var(--present); background: var(--present-bg); }
  .pbanner.absent { border-color: var(--absent); background: var(--absent-bg); }
  .pwho { font-size: 3.4rem; font-weight: 800; }
  .psub { font-size: 1.8rem; }
</style>
