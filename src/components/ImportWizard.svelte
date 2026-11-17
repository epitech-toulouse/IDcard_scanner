<script lang="ts">
  import { onMount } from "svelte";
  import Modal from "./Modal.svelte";
  import CohortSelect from "./CohortSelect.svelte";
  import { api } from "../lib/api";
  import { guard, pickFile } from "../lib/app.svelte";
  import { schoolYear } from "../lib/format";
  import type { ColumnMapping, Id, ImportReport, Preview, ProgramNode } from "../lib/types";

  let { onclose, ondone, cohortId = null }: { onclose: () => void; ondone: () => void; cohortId?: Id | null } = $props();

  let tree = $state<ProgramNode[]>([]);
  let path = $state<string | null>(null);
  let preview = $state<Preview | null>(null);
  let mapping = $state<ColumnMapping>({ student_number: null, last_name: null, first_name: null, email: null, group: null });
  let target = $state<Id | null>(null);
  let creating = $state(false);
  let newProg = $state<Id | null>(null);
  let newProgName = $state("");
  let newCohort = $state({ name: "", year: schoolYear() });
  let report = $state<ImportReport | null>(null);
  let busy = $state(false);

  const FIELDS: [keyof ColumnMapping, string, boolean][] = [
    ["student_number", "N° étudiant", true],
    ["last_name", "Nom", true],
    ["first_name", "Prénom", true],
    ["email", "Email", false],
    ["group", "Groupe(s)", false],
  ];

  onMount(async () => {
    tree = (await guard(api.tree())) ?? [];
    target = cohortId ?? tree[0]?.cohorts[0]?.id ?? null;
    creating = tree.every((p) => p.cohorts.length === 0);
    newProg = tree[0]?.id ?? null;
    await choose();
  });

  async function choose() {
    const p = await pickFile();
    if (!p) return;
    const pv = await guard(api.importPreview(p));
    if (pv) {
      path = p;
      preview = pv;
      mapping = { ...pv.mapping };
    }
  }

  let ready = $derived(
    !!preview && mapping.student_number !== null && mapping.last_name !== null && mapping.first_name !== null &&
      (creating ? !!newCohort.name.trim() && (newProg !== null || !!newProgName.trim()) : target !== null),
  );

  async function run() {
    if (!path) return;
    busy = true;
    try {
      let cid = target;
      if (creating) {
        let pid = newProg;
        if (pid === null) {
          const prog = await guard(api.createProgram(newProgName));
          if (!prog) return;
          pid = prog.id;
        }
        const co = await guard(api.createCohort(pid, newCohort.name, newCohort.year));
        if (!co) return;
        cid = co.id;
      }
      if (cid === null) return;
      const r = await guard(api.importRun(path, $state.snapshot(mapping), cid));
      if (r) {
        report = r;
        ondone();
      }
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Importer une liste d'étudiants" wide {onclose}>
  {#if report}
    <div class="card">
      <h3>Import terminé</h3>
      <p>
        <strong>{report.created}</strong> étudiant(s) créé(s), <strong>{report.updated}</strong> mis à jour,
        <strong>{report.groups_created}</strong> groupe(s) créé(s){report.skipped ? `, ${report.skipped} ligne(s) vide(s) ignorée(s)` : ""}.
      </p>
      {#if report.errors.length}
        <p class="danger">{report.errors.length} ligne(s) en erreur :</p>
        <ul class="small">{#each report.errors as e}<li>{e}</li>{/each}</ul>
      {/if}
      <p class="muted small">Les cartes s'associent ensuite au premier badge : une carte inconnue propose de choisir l'étudiant.</p>
    </div>
  {:else if !preview}
    <p>Choisissez un fichier CSV, XLSX, XLS ou ODS exporté de la scolarité (une ligne par étudiant, en-têtes en première ligne).</p>
    <div><button class="primary" onclick={choose}>Choisir un fichier…</button></div>
  {:else}
    <div class="row">
      <span class="mono small grow">{path}</span>
      <span class="muted small">{preview.total_rows} ligne(s)</span>
      <button onclick={choose}>Changer de fichier</button>
    </div>

    <h3>Colonnes</h3>
    <div class="grid3">
      {#each FIELDS as [key, label, required] (key)}
        <label class="field">
          {label}{required ? " *" : ""}
          <select
            value={mapping[key] ?? ""}
            onchange={(e) => (mapping[key] = e.currentTarget.value === "" ? null : Number(e.currentTarget.value))}
          >
            <option value="">— {required ? "à choisir" : "aucune"} —</option>
            {#each preview.headers as h, i}<option value={i}>{h || `Colonne ${i + 1}`}</option>{/each}
          </select>
        </label>
      {/each}
    </div>

    <h3>Promotion de destination</h3>
    <div class="row">
      <label class="row"><input type="radio" checked={!creating} onchange={() => (creating = false)} disabled={tree.every((p) => !p.cohorts.length)} /> Existante</label>
      <label class="row"><input type="radio" checked={creating} onchange={() => (creating = true)} /> Nouvelle</label>
    </div>
    {#if !creating}
      <CohortSelect {tree} bind:value={target} />
    {:else}
      <div class="grid3">
        <label class="field">
          Cursus
          <select value={newProg ?? ""} onchange={(e) => (newProg = e.currentTarget.value === "" ? null : Number(e.currentTarget.value))}>
            {#each tree as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
            <option value="">+ Nouveau cursus…</option>
          </select>
        </label>
        {#if newProg === null}
          <label class="field">Nom du cursus <input bind:value={newProgName} placeholder="BUT Informatique" /></label>
        {/if}
        <label class="field">Promotion <input bind:value={newCohort.name} placeholder="BUT2" /></label>
        <label class="field">Année scolaire <input bind:value={newCohort.year} /></label>
      </div>
    {/if}
    <p class="muted small">Les groupes de la colonne « Groupe(s) » (séparés par , ; / ou |) sont créés dans cette promotion. Un étudiant déjà connu (même n°) est mis à jour.</p>

    <h3>Aperçu</h3>
    <div class="table-wrap">
      <table class="small">
        <thead><tr>{#each preview.headers as h}<th>{h}</th>{/each}</tr></thead>
        <tbody>
          {#each preview.sample as r}<tr>{#each preview.headers as _, i}<td>{r[i] ?? ""}</td>{/each}</tr>{/each}
        </tbody>
      </table>
    </div>
  {/if}

  {#snippet footer()}
    {#if report}
      <button class="primary" onclick={onclose}>Fermer</button>
    {:else}
      <button onclick={onclose}>Annuler</button>
      <button class="primary" disabled={!ready || busy} onclick={run}>{busy ? "Import…" : "Importer"}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .danger { color: var(--absent); }
</style>
