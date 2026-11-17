<script lang="ts">
  import { onMount, untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import TargetPicker from "./TargetPicker.svelte";
  import { api } from "../lib/api";
  import { guard, toast } from "../lib/app.svelte";
  import { addMinutes, joinNaive, parseNaive, splitNaive, toNaive } from "../lib/format";
  import type { Id, ProgramNode, Session, SessionRow } from "../lib/types";

  let { session = null, onclose, onsaved }: {
    session?: SessionRow | null;
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
  const start0 = initial?.starts_at ?? toNaive(nextHalfHour());
  let tree = $state<ProgramNode[]>([]);
  let title = $state(initial?.title ?? "");
  let room = $state(initial?.room ?? "");
  let date = $state(splitNaive(start0).date);
  let start = $state(splitNaive(start0).time);
  let end = $state(initial ? splitNaive(initial.ends_at).time : splitNaive(addMinutes(start0, 120)).time);
  let late = $state(initial?.late_after_min ?? 10);
  let groupIds = $state<Id[]>(initial?.group_ids ?? []);
  let cohortIds = $state<Id[]>(initial?.cohort_ids ?? []);

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
    const r = session ? await guard(api.updateSession(session.id, input)) : await guard(api.createSession(input));
    if (r) {
      toast(session ? "Créneau modifié" : "Créneau créé", "success");
      onsaved(r);
    }
  }
</script>

<Modal title={session ? "Modifier le créneau" : "Nouveau créneau"} wide {onclose}>
  <div class="grid2">
    <label class="field">Intitulé * <input bind:value={title} placeholder="Ex. R3.05 Programmation système – TP" /></label>
    <label class="field">Salle <input bind:value={room} placeholder="B12" /></label>
  </div>
  <div class="grid4">
    <label class="field">Date <input type="date" bind:value={date} /></label>
    <label class="field">Début <input type="time" bind:value={start} onchange={onStartChange} /></label>
    <label class="field">Fin <input type="time" bind:value={end} /></label>
    <label class="field">Retard après (min) <input type="number" min="0" max="240" bind:value={late} /></label>
  </div>
  <div class="field">
    <span class="muted small">Étudiants attendus : cochez une promotion entière ou des groupes</span>
    <TargetPicker {tree} bind:groupIds bind:cohortIds />
  </div>

  {#snippet footer()}
    <button onclick={onclose}>Annuler</button>
    <button class="primary" disabled={!title.trim() || (!groupIds.length && !cohortIds.length)} onclick={save}>
      {session ? "Enregistrer" : "Créer le créneau"}
    </button>
  {/snippet}
</Modal>

<style>
  .grid4 { display: grid; grid-template-columns: 1.3fr 1fr 1fr 1fr; gap: 0.75rem; }
</style>
