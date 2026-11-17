<script lang="ts">
  import { onMount, untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import CohortSelect from "./CohortSelect.svelte";
  import { api, events } from "../lib/api";
  import { confirmAction, guard, toast } from "../lib/app.svelte";
  import type { Id, ProgramNode, StudentInput, StudentRow } from "../lib/types";

  let { student = null, defaultCohort = null, onclose, onsaved }: {
    student?: StudentRow | null;
    defaultCohort?: Id | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  // Le formulaire lit l'étudiant une seule fois, à l'ouverture.
  const initial = untrack(() => student);
  const initialCohort = untrack(() => defaultCohort);
  let tree = $state<ProgramNode[]>([]);
  let form = $state<StudentInput>({
    student_number: initial?.student_number ?? "",
    last_name: initial?.last_name ?? "",
    first_name: initial?.first_name ?? "",
    email: initial?.email ?? "",
    cohort_id: initial?.cohort_id ?? initialCohort,
    active: initial?.active ?? true,
  });
  let groupIds = $state<Id[]>(initial?.group_ids ?? []);
  let cards = $state<string[]>(initial?.cards ?? []);
  let manualUid = $state("");
  let waitingCard = $state(false);

  let cohortGroups = $derived(tree.flatMap((p) => p.cohorts).find((c) => c.id === form.cohort_id)?.groups ?? []);

  onMount(() => {
    guard(api.tree()).then((t) => (tree = t ?? []));
    // Badge posé pendant que la fiche est ouverte → association.
    const off = events.onCard(async (o) => {
      if (!waitingCard || !student) return;
      if (o.kind === "unknown_card" || (o.kind === "identified" && !o.student)) {
        await link(o.uid);
      } else if ("student" in o && o.student) {
        toast(`Cette carte est déjà associée à ${o.student.last_name.toUpperCase()} ${o.student.first_name}`, "warn");
      }
    });
    return () => off.then((f) => f());
  });

  async function link(uid: string) {
    if (!student) return;
    if ((await guard(api.linkCard(uid, student.id))) !== undefined) {
      cards = [...cards, uid];
      waitingCard = false;
      manualUid = "";
      toast(`Carte ${uid} associée`, "success");
      onsaved();
    }
  }

  async function unlink(uid: string) {
    if (!(await confirmAction(`Dissocier la carte ${uid} ?`))) return;
    if ((await guard(api.unlinkCard(uid))) !== undefined) {
      cards = cards.filter((c) => c !== uid);
      onsaved();
    }
  }

  async function save() {
    // Ne garder que les groupes de la promotion choisie.
    const keep = groupIds.filter((g) => cohortGroups.some((cg) => cg.id === g));
    const input = { ...$state.snapshot(form), email: form.email?.trim() || null };
    const r = student ? await guard(api.updateStudent(student.id, input, keep)) : await guard(api.createStudent(input, keep));
    if (r) {
      toast(student ? "Étudiant mis à jour" : "Étudiant ajouté", "success");
      onsaved();
      onclose();
    }
  }
</script>

<Modal title={student ? `${student.last_name.toUpperCase()} ${student.first_name}` : "Nouvel étudiant"} {onclose}>
  <div class="grid2">
    <label class="field">Nom * <input bind:value={form.last_name} /></label>
    <label class="field">Prénom * <input bind:value={form.first_name} /></label>
    <label class="field">N° étudiant * <input bind:value={form.student_number} /></label>
    <label class="field">Email <input type="email" bind:value={form.email} /></label>
  </div>
  <label class="field">Promotion <CohortSelect {tree} bind:value={form.cohort_id} allowNone /></label>
  {#if cohortGroups.length}
    <div class="field">
      <span class="muted small">Groupes</span>
      <div class="row">
        {#each cohortGroups as g (g.id)}
          <label class="row chip">
            <input
              type="checkbox"
              checked={groupIds.includes(g.id)}
              onchange={(e) => (groupIds = e.currentTarget.checked ? [...groupIds, g.id] : groupIds.filter((x) => x !== g.id))}
            />
            {g.name}
          </label>
        {/each}
      </div>
    </div>
  {/if}
  <label class="row"><input type="checkbox" bind:checked={form.active} /> Actif (décocher pour un étudiant parti : il n'est plus attendu)</label>

  {#if student}
    <div class="card cards">
      <h3>Cartes</h3>
      {#each cards as c (c)}
        <div class="row"><span class="mono grow">{c}</span><button class="link danger" onclick={() => unlink(c)}>Dissocier</button></div>
      {:else}
        <p class="muted small">Aucune carte associée.</p>
      {/each}
      <div class="row">
        {#if waitingCard}
          <span class="pulse">Posez la carte sur le lecteur…</span>
          <button onclick={() => (waitingCard = false)}>Annuler</button>
        {:else}
          <button onclick={() => (waitingCard = true)}>Associer en badgeant</button>
        {/if}
        <span class="muted small">ou</span>
        <input bind:value={manualUid} placeholder="UID (04:24:A5:…)" size="18" />
        <button disabled={!manualUid.trim()} onclick={() => link(manualUid)}>Ajouter</button>
      </div>
    </div>
  {/if}

  {#snippet footer()}
    <button onclick={onclose}>Annuler</button>
    <button class="primary" onclick={save}>Enregistrer</button>
  {/snippet}
</Modal>

<style>
  .chip { border: 1px solid var(--border); border-radius: 99px; padding: 0.15rem 0.6rem; gap: 0.35rem; }
  .cards { background: var(--surface-2); box-shadow: none; display: flex; flex-direction: column; gap: 0.5rem; }
  .pulse { color: var(--accent); font-weight: 600; animation: pulse 1.2s infinite; }
  @keyframes pulse { 50% { opacity: 0.45; } }
</style>
