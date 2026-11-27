<script lang="ts">
  // Créneaux : vue semaine (agenda) ou liste, détail du créneau sélectionné à droite.
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { confirmAction, guard, navigate, toast } from "../lib/app.svelte";
  import { exportSession } from "../lib/actions";
  import { addDays, fmtDate, fmtTime, parseNaive, startOfWeek, toNaive } from "../lib/format";
  import SessionForm from "../components/SessionForm.svelte";
  import Modal from "../components/Modal.svelte";
  import Icon from "../components/Icon.svelte";
  import { SCOPE_LABEL, STATE_LABEL, type Naive, type SeriesScope, type SessionRow } from "../lib/types";
  import { layoutDay, weekHours } from "../lib/calendar";

  let { create = false }: { create?: boolean } = $props();

  const HOUR_PX = 56;
  let weekStart = $state(startOfWeek(new Date()));
  let view = $state<"week" | "list">("week");
  let all = $state(false);
  let rows = $state<SessionRow[]>([]);
  let editing = $state<SessionRow | null | "new">(null);
  let newStart = $state<Naive | null>(null);
  let selectedId = $state<number | null>(null);
  let deleting = $state<SessionRow | null>(null);
  let now = $state(new Date());

  let selected = $derived(rows.find((r) => r.id === selectedId) ?? null);
  let weekLabel = $derived(
    `${parseNaive(weekStart).toLocaleDateString("fr-FR", { day: "numeric", month: "long" })} – ${parseNaive(
      addDays(weekStart, 6),
    ).toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" })}`,
  );

  // Vue liste : regroupement par jour, ordre chronologique.
  let byDay = $derived.by(() => {
    const m = new Map<string, SessionRow[]>();
    for (const r of [...rows].reverse()) {
      const k = r.starts_at.slice(0, 10);
      m.set(k, [...(m.get(k) ?? []), r]);
    }
    return [...m.entries()];
  });

  // Vue semaine : lundi → vendredi, plus le week-end s'il a des créneaux.
  let hours = $derived(weekHours(rows));
  let days = $derived.by(() => {
    const list = Array.from({ length: 7 }, (_, i) => addDays(weekStart, i).slice(0, 10));
    const used = new Set(rows.map((r) => r.starts_at.slice(0, 10)));
    const today = toNaive(now).slice(0, 10);
    return list
      .filter((d, i) => i < 5 || used.has(d))
      .map((d) => ({
        date: d,
        name: parseNaive(d + "T00:00:00").toLocaleDateString("fr-FR", { weekday: "short" }),
        num: Number(d.slice(8, 10)),
        today: d === today,
        events: layoutDay(rows.filter((r) => r.starts_at.slice(0, 10) === d), hours.from, HOUR_PX),
      }));
  });
  let nowTop = $derived((now.getHours() + now.getMinutes() / 60 - hours.from) * HOUR_PX);

  async function load() {
    const showAll = view === "list" && all;
    rows = (await guard(showAll ? api.listSessions() : api.listSessions(weekStart, addDays(weekStart, 7)))) ?? [];
    if (selectedId !== null && !rows.some((r) => r.id === selectedId)) selectedId = null;
  }
  onMount(() => {
    if (create) {
      editing = "new";
      history.replaceState(null, "", "#/creneaux");
    }
    load();
    const t = setInterval(() => (now = new Date()), 60_000);
    return () => clearInterval(t);
  });

  function shift(weeks: number) {
    weekStart = addDays(weekStart, 7 * weeks);
    all = false;
    load();
  }

  /** Clic dans une colonne vide : nouveau créneau à la demi-heure cliquée. */
  function createAt(e: MouseEvent, date: string) {
    if (e.target !== e.currentTarget) return;
    const y = e.offsetY / HOUR_PX + hours.from;
    const h = Math.floor(y), m = y - h >= 0.5 ? 30 : 0;
    newStart = `${date}T${String(h).padStart(2, "0")}:${m ? "30" : "00"}:00`;
    editing = "new";
  }

  async function duplicate(s: SessionRow) {
    const r = await guard(api.duplicateSession(s.id, addDays(s.starts_at, 7)));
    if (r) {
      toast(`Dupliqué au ${fmtDate(r.starts_at)}`, "success");
      load();
    }
  }

  async function remove(s: SessionRow) {
    if (s.series_id !== null) {
      deleting = s;
      return;
    }
    const has = s.counts.present + s.counts.late + s.counts.excused > 0;
    if (!(await confirmAction(`Supprimer le créneau « ${s.title} » du ${fmtDate(s.starts_at)} ?${has ? "\n\nLes présences enregistrées seront perdues." : ""}`))) return;
    if ((await guard(api.deleteSession(s.id))) !== undefined) {
      selectedId = null;
      load();
    }
  }

  async function removeScoped(scope: SeriesScope) {
    if (!deleting) return;
    const n = await guard(api.deleteSessionScoped(deleting.id, scope));
    deleting = null;
    if (n !== undefined) {
      toast(n > 1 ? `${n} créneaux supprimés` : "Créneau supprimé", "success");
      selectedId = null;
      load();
    }
  }

  const cta = (s: SessionRow) => (s.state === "closed" ? "Voir la feuille" : s.state === "open" ? "Reprendre l'émargement" : "Émarger");
  const here = (s: SessionRow) => s.counts.present + s.counts.late;
</script>

<div class="page-head">
  <div class="row head">
    <h1>Créneaux</h1>
    <div class="row nav">
      <button class="icon" aria-label="Semaine précédente" onclick={() => shift(-1)}><Icon name="chevronLeft" stroke={2} /></button>
      <button onclick={() => { weekStart = startOfWeek(new Date()); all = false; load(); }}>Cette semaine</button>
      <button class="icon" aria-label="Semaine suivante" onclick={() => shift(1)}><Icon name="chevronRight" stroke={2} /></button>
    </div>
    <strong>{view === "list" && all ? "Tous les créneaux" : weekLabel}</strong>
  </div>
  <div class="row">
    <div class="tabs" role="tablist" aria-label="Vue">
      <button role="tab" aria-selected={view === "week"} class:on={view === "week"} onclick={() => { view = "week"; all = false; load(); }}>Semaine</button>
      <button role="tab" aria-selected={view === "list"} class:on={view === "list"} onclick={() => (view = "list")}>Liste</button>
    </div>
    {#if view === "list"}
      <label class="row small"><input type="checkbox" bind:checked={all} onchange={load} /> Toutes les semaines</label>
    {/if}
    <button class="primary" onclick={() => { newStart = null; editing = "new"; }}><Icon name="plus" size={16} stroke={2.2} />Nouveau créneau</button>
  </div>
</div>

{#if view === "week"}
  <div class="layout">
    <section class="card flush cal" aria-label="Semaine du {weekLabel}">
      <div class="legend small">
        <span><i class="sw planned"></i>Prévu</span>
        <span><i class="sw open"></i>En cours</span>
        <span><i class="sw closed"></i>Clos</span>
        <span class="grow"></span>
        <span class="muted">Cliquez sur une plage libre pour créer un créneau</span>
      </div>
      <div class="cal-scroll">
        <div class="grid" style:--cols={days.length} style:--h="{(hours.to - hours.from) * HOUR_PX}px">
          <span></span>
          {#each days as d (d.date)}
            <div class="dhead" class:today={d.today}><span class="dname">{d.name}</span><span class="dnum">{d.num}</span></div>
          {/each}
          <div class="hours">
            {#each Array.from({ length: hours.to - hours.from - 1 }, (_, i) => hours.from + i + 1) as h (h)}
              <span style:top="{(h - hours.from) * HOUR_PX}px">{h}:00</span>
            {/each}
          </div>
          {#each days as d (d.date)}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="col" class:today={d.today} style:--hp="{HOUR_PX}px" onclick={(e) => createAt(e, d.date)}>
              {#each d.events as e (e.row.id)}
                <button
                  class="ev {e.row.state}"
                  class:sel={selectedId === e.row.id}
                  style:top="{e.top}px"
                  style:height="{e.height}px"
                  style:left="calc({e.left}% + 4px)"
                  style:width="calc({e.width}% - 8px)"
                  onclick={() => (selectedId = e.row.id)}
                  ondblclick={() => navigate(`emargement/${e.row.id}`)}
                >
                  <span class="ev-time">{fmtTime(e.row.starts_at)} – {fmtTime(e.row.ends_at)}{#if e.row.series_id !== null}<span class="rep" title="Créneau répété"><Icon name="repeat" size={12} stroke={2.2} /></span>{/if}</span>
                  <span class="ev-title">{e.row.title}</span>
                  <span class="ev-sub">{e.row.room ? `${e.row.room} · ` : ""}{e.row.state === "planned" ? "—" : here(e.row)}/{e.row.counts.expected}</span>
                </button>
              {/each}
              {#if d.today && nowTop > 0 && nowTop < (hours.to - hours.from) * HOUR_PX}
                <div class="now" style:top="{nowTop}px" aria-hidden="true"></div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    </section>

    <aside class="card detail" aria-label="Créneau sélectionné">
      {#if selected}
        {@const s = selected}
        <div>
          <span class="pill {s.state}">{STATE_LABEL[s.state]}</span>
          <h2>{s.title}</h2>
          <p class="muted"><span class="cap">{fmtDate(s.starts_at)}</span> · {fmtTime(s.starts_at)} – {fmtTime(s.ends_at)}</p>
        </div>
        <dl>
          <dt>Salle</dt><dd>{s.room ?? "—"}</dd>
          <dt>Attendus</dt><dd>{s.targets}</dd>
          <dt>Présents</dt><dd><strong>{here(s)}</strong> / {s.counts.expected}{s.counts.late ? ` · ${s.counts.late} retard(s)` : ""}</dd>
          {#if s.counts.absent || s.counts.excused}<dt>Absents</dt><dd>{s.counts.absent}{s.counts.excused ? ` · ${s.counts.excused} justifié(s)` : ""}</dd>{/if}
          <dt>Retard</dt><dd>après {s.late_after_min} min</dd>
          {#if s.series_id !== null}<dt>Répétition</dt><dd class="rep-dd"><Icon name="repeat" size={14} stroke={2} />Fait partie d'une série</dd>{/if}
        </dl>
        <div class="actions">
          <button class="primary big" onclick={() => navigate(`emargement/${s.id}`)}>{cta(s)}</button>
          <div class="row two">
            <button onclick={() => (editing = s)}>Modifier</button>
            <button title="Même créneau la semaine suivante" onclick={() => duplicate(s)}>Dupliquer à +7 j</button>
          </div>
          <div class="row two">
            <button onclick={() => exportSession(s, "xlsx")}>Excel</button>
            <button onclick={() => exportSession(s, "csv")}>CSV</button>
          </div>
          <button class="link danger" onclick={() => remove(s)}>Supprimer le créneau</button>
        </div>
      {:else}
        <div class="placeholder">
          <Icon name="calendar" size={28} />
          <p>Sélectionnez un créneau pour voir son détail.</p>
          <p class="small muted">Double-clic : ouvrir l'émargement.</p>
        </div>
      {/if}
    </aside>
  </div>
{:else}
  {#if rows.length === 0}
    <div class="card empty">
      <p>Aucun créneau {all ? "" : "cette semaine"}.</p>
      <button class="primary" onclick={() => (editing = "new")}>Créer un créneau</button>
    </div>
  {/if}
  <div class="stack">
    {#each byDay as [day, list] (day)}
      <section>
        <h3 class="day">{fmtDate(day + "T00:00:00")}</h3>
        <div class="table-wrap">
          <table>
            <tbody>
              {#each list as s (s.id)}
                <tr>
                  <td class="time">{fmtTime(s.starts_at)} – {fmtTime(s.ends_at)}</td>
                  <td>
                    <strong>{s.title}</strong>{#if s.room}<span class="muted"> · {s.room}</span>{/if}
                    {#if s.series_id !== null}<span class="rep muted" title="Créneau répété"><Icon name="repeat" size={13} stroke={2} /></span>{/if}
                    <div class="small muted">{s.targets}</div>
                  </td>
                  <td class="small"><strong>{here(s)}</strong>/{s.counts.expected} présents</td>
                  <td><span class="pill {s.state}">{STATE_LABEL[s.state]}</span></td>
                  <td class="actions">
                    <button class:primary={s.state !== "closed"} onclick={() => navigate(`emargement/${s.id}`)}>{s.state === "closed" ? "Voir" : "Émarger"}</button>
                    <button class="link" onclick={() => (editing = s)}>Modifier</button>
                    <button class="link" title="Même créneau la semaine suivante" onclick={() => duplicate(s)}>+7 j</button>
                    <button class="link danger" aria-label="Supprimer" onclick={() => remove(s)}><Icon name="close" size={16} stroke={2} /></button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>
    {/each}
  </div>
{/if}

{#if deleting}
  {@const d = deleting}
  <Modal title="Supprimer un créneau répété" onclose={() => (deleting = null)}>
    <p><strong>{d.title}</strong> · <span class="cap">{fmtDate(d.starts_at)}</span> fait partie d'une série.</p>
    <p class="small muted">Seuls les créneaux encore prévus de la série sont supprimés : ceux déjà ouverts ou clos gardent leurs présences.</p>
    <div class="scope-btns">
      {#each ["one", "following", "all"] as const as sc (sc)}
        <button class:danger={sc !== "one"} onclick={() => removeScoped(sc)}>{SCOPE_LABEL[sc]}</button>
      {/each}
    </div>
    {#snippet footer()}<button onclick={() => (deleting = null)}>Annuler</button>{/snippet}
  </Modal>
{/if}

{#if editing}
  <SessionForm
    session={editing === "new" ? null : editing}
    startAt={editing === "new" ? newStart : null}
    onclose={() => (editing = null)}
    onsaved={(s) => {
      editing = null;
      if (!all) weekStart = startOfWeek(parseNaive(s.starts_at));
      selectedId = s.id;
      load();
    }}
  />
{/if}

<style>
  .head { gap: 1rem; }
  .head h1 { margin: 0 0.4rem 0 0; }
  .nav { gap: 0.3rem; }
  .cap { text-transform: capitalize; }
  .layout { display: flex; flex-wrap: wrap; gap: 1.2rem; align-items: flex-start; }
  .cal { flex: 999 1 640px; min-width: 0; }
  .detail { flex: 1 1 280px; min-width: 0; display: flex; flex-direction: column; gap: 1.1rem; position: sticky; top: 0; }

  .legend { display: flex; flex-wrap: wrap; gap: 1rem; align-items: center; padding: 0.75rem 1.1rem; border-bottom: 1px solid var(--border); color: var(--text-2); }
  .sw { display: inline-block; width: 0.7rem; height: 0.7rem; border-radius: 3px; margin-right: 0.4rem; vertical-align: -1px; border: 1px solid; }
  .sw.planned { background: var(--excused-bg); border-color: var(--excused-bd); }
  .sw.open { background: var(--present-bg); border-color: var(--present-bd); }
  .sw.closed { background: var(--closed-bg); border-color: var(--closed-bd); }

  .cal-scroll { overflow-x: auto; }
  .grid { display: grid; grid-template-columns: 3.5rem repeat(var(--cols), minmax(8.5rem, 1fr)); min-width: 40rem; }
  .dhead { display: flex; align-items: center; gap: 0.5rem; padding: 0.6rem 0.8rem; border-left: 1px solid var(--border); border-bottom: 1px solid var(--border); }
  .dname { font-size: 0.8rem; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--muted); }
  .dnum { font-size: 1.15rem; font-weight: 700; min-width: 2rem; height: 2rem; border-radius: 50%; display: inline-grid; place-items: center; }
  .dhead.today .dnum { background: var(--accent); color: #fff; }
  .hours { position: relative; height: var(--h); border-bottom: 1px solid var(--border); }
  .hours span { position: absolute; right: 0.5rem; transform: translateY(-50%); font-size: 0.75rem; color: var(--muted); }
  .col {
    position: relative; height: var(--h); border-left: 1px solid var(--border); border-bottom: 1px solid var(--border); cursor: copy;
    background-image: repeating-linear-gradient(to bottom, transparent 0 calc(var(--hp) - 1px), var(--border) calc(var(--hp) - 1px) var(--hp));
  }
  .col.today { background-color: color-mix(in srgb, var(--accent-soft) 45%, transparent); }
  .ev {
    position: absolute; box-sizing: border-box; display: flex; flex-direction: column; align-items: flex-start; justify-content: flex-start;
    gap: 1px; padding: 0.35rem 0.55rem; min-height: 0; border-radius: 8px; text-align: left; overflow: hidden; white-space: normal;
    color: var(--text); font-weight: 400; cursor: pointer; border: 1px solid;
  }
  .ev.planned { background: var(--excused-bg); border-color: var(--excused-bd); }
  .ev.open { background: var(--present-bg); border-color: var(--present-bd); }
  .ev.closed { background: var(--closed-bg); border-color: var(--closed-bd); }
  .ev:hover:not(:disabled) { filter: brightness(0.97); }
  .ev.planned:hover:not(:disabled) { background: var(--excused-bg); }
  .ev.open:hover:not(:disabled) { background: var(--present-bg); }
  .ev.closed:hover:not(:disabled) { background: var(--closed-bg); }
  .ev.sel { outline: 2px solid var(--text); outline-offset: -1px; }
  .ev-time { font-size: 0.78rem; font-weight: 600; display: flex; align-items: center; gap: 0.3rem; }
  .rep { display: inline-flex; vertical-align: -2px; margin-left: 0.3rem; }
  .ev-time .rep { margin-left: 0; }
  .rep-dd { display: flex; align-items: center; gap: 0.35rem; }
  .scope-btns { display: flex; flex-direction: column; gap: 0.5rem; }
  .scope-btns button { justify-content: flex-start; }
  .ev.planned .ev-time { color: var(--excused); }
  .ev.open .ev-time { color: var(--present); }
  .ev.closed .ev-time { color: var(--text-2); }
  .ev-title { font-size: 0.88rem; font-weight: 600; line-height: 1.25; }
  .ev-sub { font-size: 0.78rem; color: var(--text-2); }
  .now { position: absolute; left: 0; right: 0; height: 2px; background: var(--now); pointer-events: none; }
  .now::before { content: ""; position: absolute; left: -5px; top: -4px; width: 10px; height: 10px; border-radius: 50%; background: var(--now); }

  .detail h2 { margin: 0.6rem 0 0.1rem; font-size: 1.25rem; font-weight: 700; }
  .detail p { margin: 0; }
  dl { margin: 0; display: grid; grid-template-columns: 5.5rem 1fr; gap: 0.5rem 0.8rem; font-size: 0.93rem; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  .actions { display: flex; flex-direction: column; gap: 0.5rem; }
  .two > button { flex: 1; }
  .placeholder { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 0.4rem; padding: 2rem 0.5rem; color: var(--muted); }

  .day { text-transform: capitalize; margin: 0 0 0.4rem; color: var(--muted); }
  .time { white-space: nowrap; width: 8rem; }
</style>
