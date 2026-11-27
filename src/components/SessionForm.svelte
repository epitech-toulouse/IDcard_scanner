<script lang="ts">
  import { onMount, untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import TargetPicker from "./TargetPicker.svelte";
  import { api } from "../lib/api";
  import { guard, toast } from "../lib/app.svelte";
  import { addMinutes, joinNaive, parseNaive, splitNaive, toNaive } from "../lib/format";
  import Icon from "./Icon.svelte";
  import { SCOPE_LABEL, type Id, type Naive, type ProgramNode, type Recurrence, type SeriesPlan, type SeriesScope, type Session, type SessionRow } from "../lib/types";

  let { session = null, startAt = null, onclose, onsaved }: {
    session?: SessionRow | null;
    /** Début proposé pour un nouveau créneau (clic dans le calendrier). */
    startAt?: Naive | null;
    onclose: () => void;
    onsaved: (s: Session) => void;
  } = $props();

  function nextHalfHour(): Date {
    const d = new Date();
    d.setSeconds(0, 0);
    d.setMinutes(d.getMinutes() < 30 ? 30 : 60);
    return d;
  }

  // Le formulaire lit le créneau une seule fois, à l'ouverture.
  const initial = untrack(() => session);
  const start0 = initial?.starts_at ?? untrack(() => startAt) ?? toNaive(nextHalfHour());
  let tree = $state<ProgramNode[]>([]);
  let title = $state(initial?.title ?? "");
  let room = $state(initial?.room ?? "");
  let date = $state(splitNaive(start0).date);
  let start = $state(splitNaive(start0).time);
  let end = $state(initial ? splitNaive(initial.ends_at).time : splitNaive(addMinutes(start0, 120)).time);
  let late = $state(initial?.late_after_min ?? 10);
  let groupIds = $state<Id[]>(initial?.group_ids ?? []);
  let cohortIds = $state<Id[]>(initial?.cohort_ids ?? []);

  // ---- Répétition (création seulement) ----
  const WEEKDAYS = [
    { n: 1, short: "L", label: "Lundi" }, { n: 2, short: "M", label: "Mardi" }, { n: 3, short: "M", label: "Mercredi" },
    { n: 4, short: "J", label: "Jeudi" }, { n: 5, short: "V", label: "Vendredi" }, { n: 6, short: "S", label: "Samedi" },
    { n: 7, short: "D", label: "Dimanche" },
  ];
  const isoDay = (d: string) => ((parseNaive(d + "T00:00:00").getDay() + 6) % 7) + 1;
  let repeat = $state<0 | 1 | 2>(0); // 0 = une seule fois, sinon toutes les N semaines
  let weekdays = $state<number[]>([isoDay(splitNaive(start0).date)]);
  let endKind = $state<"until" | "count">("until");
  let until = $state(splitNaive(addMinutes(start0, 60 * 24 * 27)).date);
  let count = $state(10);
  let skipHolidays = $state(true);
  let plan = $state<SeriesPlan | null>(null);
  let planError = $state("");
  let rule = $derived<Recurrence>({
    weekdays: [...weekdays].sort(),
    interval_weeks: repeat || 1,
    until: endKind === "until" ? until : null,
    count: endKind === "count" ? Number(count) : null,
    skip_holidays: skipHolidays,
  });
  let seq = 0;
  $effect(() => {
    if (session || !repeat || !date) {
      plan = null;
      planError = "";
      return;
    }
    const n = ++seq;
    api.previewSessionSeries(joinNaive(date, start), rule).then(
      (p) => { if (n === seq) { plan = p; planError = ""; } },
      (e) => { if (n === seq) { plan = null; planError = String(e); } },
    );
  });
  function toggleDay(n: number) {
    weekdays = weekdays.includes(n) ? weekdays.filter((d) => d !== n) : [...weekdays, n];
  }
  const fmtDay = (d: string) => parseNaive(d + "T00:00:00").toLocaleDateString("fr-FR", { weekday: "short", day: "numeric", month: "short" });

  // ---- Modification d'un créneau d'une série ----
  let scope = $state<SeriesScope>("following");
  const SCOPE_HINT: Record<SeriesScope, string> = {
    one: "Les autres créneaux ne changent pas ; celui-ci sort de la série.",
    following: "Mêmes horaires, salle et groupes pour les créneaux prévus qui suivent.",
    all: "Tous les créneaux encore prévus de la série, y compris les précédents.",
  };

  onMount(async () => {
    tree = (await guard(api.tree())) ?? [];
    if (!session) {
      const s = await guard(api.getSettings());
      if (s) {
        late = s.default_late_min;
        end = splitNaive(addMinutes(joinNaive(date, start), s.default_duration_min)).time;
      }
    }
  });

  // Garder la durée quand on déplace l'heure de début.
  let prevStart = untrack(() => start);
  function onStartChange() {
    const dur = (parseNaive(joinNaive(date, end)).getTime() - parseNaive(joinNaive(date, prevStart)).getTime()) / 60000;
    if (dur > 0) end = splitNaive(addMinutes(joinNaive(date, start), dur)).time;
    prevStart = start;
  }

  async function save() {
    const input = {
      title,
      room: room.trim() || null,
      starts_at: joinNaive(date, start),
      ends_at: joinNaive(date, end),
      late_after_min: Number(late),
      group_ids: groupIds,
      cohort_ids: cohortIds,
    };
    if (session) {
      const r = await guard(session.series_id !== null ? api.updateSessionScoped(session.id, input, scope) : api.updateSession(session.id, input));
      if (r) {
        toast(session.series_id !== null && scope !== "one" ? "Créneaux de la série modifiés" : "Créneau modifié", "success");
        onsaved(r);
      }
    } else if (repeat) {
      const list = await guard(api.createSessionSeries(input, rule));
      if (list?.length) {
        toast(`${list.length} créneaux créés`, "success");
        onsaved(list[0]);
      }
    } else {
      const r = await guard(api.createSession(input));
      if (r) {
        toast("Créneau créé", "success");
        onsaved(r);
      }
    }
  }
</script>

<Modal title={session ? "Modifier le créneau" : "Nouveau créneau"} wide {onclose}>
  <div class="grid2">
    <label class="field">Intitulé * <input bind:value={title} placeholder="Ex. R3.05 Programmation système – TP" /></label>
    <label class="field">Salle <input bind:value={room} placeholder="B12" /></label>
  </div>
  <div class="grid4">
    <label class="field">{repeat ? "À partir du" : "Date"} <input type="date" bind:value={date} /></label>
    <label class="field">Début <input type="time" bind:value={start} onchange={onStartChange} /></label>
    <label class="field">Fin <input type="time" bind:value={end} /></label>
    <label class="field">Retard après (min) <input type="number" min="0" max="240" bind:value={late} /></label>
  </div>
  <div class="field">
    <span class="muted small">Étudiants attendus : cochez une promotion entière ou des groupes</span>
    <TargetPicker {tree} bind:groupIds bind:cohortIds />
  </div>

  {#if !session}
    <section class="box" aria-labelledby="rep-title">
      <h3 id="rep-title"><Icon name="repeat" size={18} stroke={2} />Répétition</h3>
      <div class="tabs" role="radiogroup" aria-label="Fréquence">
        <button role="radio" aria-checked={repeat === 0} class:on={repeat === 0} onclick={() => (repeat = 0)}>Une seule fois</button>
        <button role="radio" aria-checked={repeat === 1} class:on={repeat === 1} onclick={() => (repeat = 1)}>Chaque semaine</button>
        <button role="radio" aria-checked={repeat === 2} class:on={repeat === 2} onclick={() => (repeat = 2)}>Une semaine sur deux</button>
      </div>
      {#if repeat}
        <div class="sub">
          <div class="row">
            <span class="label grow">Jours</span>
            <button class="link" onclick={() => (weekdays = [1, 2, 3, 4, 5])}>Lun. → ven.</button>
            <button class="link" onclick={() => (weekdays = [1, 2, 3, 4, 5, 6, 7])}>Tous les jours</button>
          </div>
          <div class="days">
            {#each WEEKDAYS as w (w.n)}
              <button class="day" class:on={weekdays.includes(w.n)} aria-pressed={weekdays.includes(w.n)} aria-label={w.label} title={w.label} onclick={() => toggleDay(w.n)}>{w.short}</button>
            {/each}
          </div>
        </div>
        <div class="sub">
          <span class="label">Jusqu'à</span>
          <label class="row choice"><input type="radio" bind:group={endKind} value="until" /> Le <input type="date" bind:value={until} disabled={endKind !== "until"} /></label>
          <label class="row choice"><input type="radio" bind:group={endKind} value="count" /> Après <input type="number" min="1" max="200" bind:value={count} disabled={endKind !== "count"} class="num" /> créneaux</label>
        </div>
        <label class="row choice"><input type="checkbox" bind:checked={skipHolidays} /> Sauter les jours fériés</label>
        <div class="preview" aria-live="polite">
          {#if planError}
            <span class="err">{planError}</span>
          {:else if plan}
            <p><strong class="count">{plan.dates.length}</strong> créneau{plan.dates.length > 1 ? "x" : ""} seront créés{plan.skipped.length ? ` · ${plan.skipped.length} férié${plan.skipped.length > 1 ? "s" : ""} sauté${plan.skipped.length > 1 ? "s" : ""} (${plan.skipped.map(fmtDay).join(", ")})` : ""}</p>
            <div class="chips">
              {#each plan.dates.slice(0, 12) as d (d)}<span class="chip">{fmtDay(d)}</span>{/each}
              {#if plan.dates.length > 12}<span class="muted small">… et {plan.dates.length - 12} autres</span>{/if}
            </div>
          {/if}
        </div>
      {/if}
    </section>
  {:else if session.series_id !== null}
    <section class="box" aria-labelledby="scope-title">
      <h3 id="scope-title"><Icon name="repeat" size={18} stroke={2} />Ce créneau fait partie d'une série</h3>
      <div class="scopes" role="radiogroup" aria-label="Appliquer à">
        {#each ["one", "following", "all"] as const as sc (sc)}
          <label class="scope" class:on={scope === sc}>
            <input type="radio" bind:group={scope} value={sc} />
            <span><strong>{SCOPE_LABEL[sc]}</strong><span class="small muted">{SCOPE_HINT[sc]}</span></span>
          </label>
        {/each}
      </div>
      <p class="small muted">Les créneaux déjà ouverts ou clos ne sont jamais modifiés : leurs présences restent telles quelles.</p>
    </section>
  {/if}

  {#snippet footer()}
    <button onclick={onclose}>Annuler</button>
    <button
      class="primary"
      disabled={!title.trim() || (!groupIds.length && !cohortIds.length) || (!session && repeat > 0 && !plan)}
      onclick={save}
    >
      {#if session}Enregistrer{:else if repeat && plan}Créer les {plan.dates.length} créneaux{:else}Créer le créneau{/if}
    </button>
  {/snippet}
</Modal>

<style>
  .grid4 { display: grid; grid-template-columns: 1.3fr 1fr 1fr 1fr; gap: 0.9rem; }
  .box { border: 1px solid var(--border); border-radius: 12px; padding: 1rem 1.1rem; display: flex; flex-direction: column; gap: 0.9rem; }
  .box h3 { display: flex; align-items: center; gap: 0.5rem; margin: 0; color: var(--text); }
  .box h3 :global(svg) { color: var(--accent); }
  .sub { display: flex; flex-direction: column; gap: 0.5rem; }
  .label { font-size: 0.93rem; font-weight: 500; color: var(--text-2); }
  .days { display: flex; gap: 0.5rem; flex-wrap: wrap; }
  .day { width: 2.9rem; min-height: 2.9rem; border-radius: 50%; padding: 0; font-weight: 700; color: var(--text-2); }
  .day.on, .day.on:hover:not(:disabled) { background: var(--accent); border-color: var(--accent); color: #fff; }
  .choice { gap: 0.5rem; }
  .num { width: 5rem; }
  .preview { background: var(--surface-3); border: 1px solid var(--border); border-radius: 10px; padding: 0.8rem 1rem; display: flex; flex-direction: column; gap: 0.5rem; }
  .preview:empty { display: none; }
  .preview p { margin: 0; }
  .count { font-size: 1.3rem; font-weight: 800; margin-right: 0.2rem; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.35rem; align-items: center; }
  .chip { font-size: 0.85rem; background: var(--surface); border: 1px solid var(--border); border-radius: 6px; padding: 0.15rem 0.55rem; text-transform: capitalize; }
  .err { color: var(--absent); }
  .scopes { display: flex; flex-direction: column; gap: 0.45rem; }
  .scope { display: flex; align-items: flex-start; gap: 0.7rem; padding: 0.7rem 0.9rem; border: 1px solid var(--border); border-radius: 10px; cursor: pointer; }
  .scope > span { display: flex; flex-direction: column; }
  .scope input { margin-top: 0.2rem; }
  .scope.on { border-color: var(--accent); background: var(--accent-soft); }
  .box > p { margin: 0; }
</style>
