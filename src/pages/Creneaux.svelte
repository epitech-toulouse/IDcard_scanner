<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { confirmAction, guard, navigate, toast } from "../lib/app.svelte";
  import { exportSessionXlsx } from "../lib/actions";
  import { addDays, fmtDate, fmtTime, parseNaive, startOfWeek } from "../lib/format";
  import SessionForm from "../components/SessionForm.svelte";
  import { STATE_LABEL, type SessionRow } from "../lib/types";

  let weekStart = $state(startOfWeek(new Date()));
  let all = $state(false);
  let rows = $state<SessionRow[]>([]);
  let editing = $state<SessionRow | null | "new">(null);

  let weekLabel = $derived(
    `Semaine du ${parseNaive(weekStart).toLocaleDateString("fr-FR", { day: "numeric", month: "long" })} au ${parseNaive(
      addDays(weekStart, 6),
    ).toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" })}`,
  );

  // Regroupement par jour, ordre chronologique.
  let byDay = $derived.by(() => {
    const m = new Map<string, SessionRow[]>();
    for (const r of [...rows].reverse()) {
      const k = r.starts_at.slice(0, 10);
      m.set(k, [...(m.get(k) ?? []), r]);
    }
    return [...m.entries()];
  });

  async function load() {
    rows = (await guard(all ? api.listSessions() : api.listSessions(weekStart, addDays(weekStart, 7)))) ?? [];
  }
  onMount(load);

  function shift(weeks: number) {
    weekStart = addDays(weekStart, 7 * weeks);
    all = false;
    load();
  }

  async function duplicate(s: SessionRow) {
    const r = await guard(api.duplicateSession(s.id, addDays(s.starts_at, 7)));
    if (r) {
      toast(`Dupliqué au ${fmtDate(r.starts_at)}`, "success");
      load();
    }
  }

  async function remove(s: SessionRow) {
    const has = s.counts.present + s.counts.late + s.counts.excused > 0;
    if (!(await confirmAction(`Supprimer le créneau « ${s.title} » du ${fmtDate(s.starts_at)} ?${has ? "\n\nLes présences enregistrées seront perdues." : ""}`))) return;
    if ((await guard(api.deleteSession(s.id))) !== undefined) load();
  }
</script>

<div class="page-head">
  <div>
    <h1>Créneaux</h1>
    <p class="muted">{all ? "Tous les créneaux" : weekLabel}</p>
  </div>
  <div class="row">
    <button onclick={() => shift(-1)}>◀</button>
    <button onclick={() => { weekStart = startOfWeek(new Date()); all = false; load(); }}>Cette semaine</button>
    <button onclick={() => shift(1)}>▶</button>
    <button class:primary={all} onclick={() => { all = !all; load(); }}>Tous</button>
    <button class="primary" onclick={() => (editing = "new")}>Nouveau créneau</button>
  </div>
</div>

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
              {@const done = s.counts.present + s.counts.late}
              <tr>
                <td class="time">{fmtTime(s.starts_at)} – {fmtTime(s.ends_at)}</td>
                <td>
                  <strong>{s.title}</strong>{#if s.room}<span class="muted"> · {s.room}</span>{/if}
                  <div class="small muted">{s.targets}</div>
                </td>
                <td class="small">
                  <strong>{done}</strong>/{s.counts.expected} présents
                  {#if s.counts.late}<span class="pill late">{s.counts.late} retard{s.counts.late > 1 ? "s" : ""}</span>{/if}
                  {#if s.counts.absent}<span class="pill absent">{s.counts.absent} abs.</span>{/if}
                  {#if s.counts.excused}<span class="pill excused">{s.counts.excused} just.</span>{/if}
                </td>
                <td><span class="pill {s.state}">{STATE_LABEL[s.state]}</span></td>
                <td class="actions">
                  <button class:primary={s.state !== "closed"} onclick={() => navigate(`emargement/${s.id}`)}>
                    {s.state === "closed" ? "Voir" : "Émarger"}
                  </button>
                  <button class="link" onclick={() => (editing = s)}>Modifier</button>
                  <button class="link" title="Même créneau la semaine suivante" onclick={() => duplicate(s)}>+7 j</button>
                  <button class="link" onclick={() => exportSessionXlsx(s)}>Excel</button>
                  <button class="link danger" onclick={() => remove(s)}>✕</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/each}
</div>

{#if editing}
  <SessionForm
    session={editing === "new" ? null : editing}
    onclose={() => (editing = null)}
    onsaved={(s) => {
      editing = null;
      if (!all) weekStart = startOfWeek(parseNaive(s.starts_at));
      load();
    }}
  />
{/if}

<style>
  .day { text-transform: capitalize; margin: 0 0 0.4rem; color: var(--muted); }
  .time { white-space: nowrap; font-variant-numeric: tabular-nums; width: 8rem; }
</style>
