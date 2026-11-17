<script lang="ts">
  import { untrack } from "svelte";
  import { api } from "../lib/api";
  import { confirmAction, guard, route, toast } from "../lib/app.svelte";
  import CohortSelect from "../components/CohortSelect.svelte";
  import StudentForm from "../components/StudentForm.svelte";
  import ImportWizard from "../components/ImportWizard.svelte";
  import type { Id, ProgramNode, StudentRow } from "../lib/types";

  let tree = $state<ProgramNode[]>([]);
  let rows = $state<StudentRow[]>([]);
  let cohortId = $state<Id | null>(route.parts[1] ? Number(route.parts[1]) : null);
  let groupId = $state<Id | null>(null);
  let search = $state("");
  let includeInactive = $state(false);
  let editing = $state<StudentRow | null | "new">(null);
  let importing = $state(false);

  let groups = $derived(tree.flatMap((p) => p.cohorts).find((c) => c.id === cohortId)?.groups ?? []);
  let withoutCard = $derived(rows.filter((r) => r.cards.length === 0).length);

  async function load() {
    tree = (await guard(api.tree())) ?? [];
    if (groupId !== null && !groups.some((g) => g.id === groupId)) groupId = null;
    rows =
      (await guard(
        api.listStudents({ cohort_id: cohortId, group_id: groupId, search: search || null, include_inactive: includeInactive }),
      )) ?? [];
  }

  // Recharge au changement de promotion (et au premier affichage).
  $effect(() => {
    void cohortId;
    untrack(load);
  });
  let timer: ReturnType<typeof setTimeout>;
  function reloadSoon() {
    clearTimeout(timer);
    timer = setTimeout(load, 200);
  }

  async function remove(s: StudentRow) {
    if (!(await confirmAction(`Supprimer ${s.last_name.toUpperCase()} ${s.first_name} et tout son historique de présence ?\n\nPour un étudiant parti, préférez le passer « inactif ».`))) return;
    if ((await guard(api.deleteStudent(s.id))) !== undefined) {
      toast("Étudiant supprimé", "success");
      load();
    }
  }
</script>

<div class="page-head">
  <div>
    <h1>Étudiants</h1>
    <p class="muted">{rows.length} étudiant(s){withoutCard ? ` · ${withoutCard} sans carte` : ""}</p>
  </div>
  <div class="row">
    <button onclick={() => (importing = true)}>Importer CSV / Excel…</button>
    <button class="primary" onclick={() => (editing = "new")}>Ajouter un étudiant</button>
  </div>
</div>

<div class="row filters">
  <input class="grow" bind:value={search} oninput={reloadSoon} placeholder="Rechercher (nom, prénom, n°)…" />
  <CohortSelect {tree} bind:value={cohortId} allowNone noneLabel="Toutes les promotions" />
  <select
    value={groupId ?? ""}
    disabled={!groups.length}
    onchange={(e) => { groupId = e.currentTarget.value === "" ? null : Number(e.currentTarget.value); load(); }}
  >
    <option value="">Tous les groupes</option>
    {#each groups as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
  </select>
  <label class="row small"><input type="checkbox" bind:checked={includeInactive} onchange={load} /> inactifs</label>
</div>

{#if rows.length === 0}
  <div class="card empty">
    <p>Aucun étudiant{search || cohortId ? " pour ce filtre" : ""}.</p>
    <p class="small">Importez la liste exportée de la scolarité (CSV / Excel), puis associez les cartes au premier badge.</p>
  </div>
{:else}
  <div class="table-wrap">
    <table>
      <thead>
        <tr><th>Nom</th><th>Prénom</th><th>N° étudiant</th><th>Promotion</th><th>Groupes</th><th>Carte</th><th></th></tr>
      </thead>
      <tbody>
        {#each rows as s (s.id)}
          <tr class:inactive={!s.active}>
            <td><strong>{s.last_name.toUpperCase()}</strong></td>
            <td>{s.first_name}</td>
            <td class="mono">{s.student_number}</td>
            <td class="small">{s.cohort_label ?? "—"}</td>
            <td>{s.groups}</td>
            <td>{#if s.cards.length}<span class="pill present">✓ {s.cards.length > 1 ? s.cards.length : ""}</span>{:else}<span class="pill">aucune</span>{/if}</td>
            <td class="actions">
              <button class="link" onclick={() => (editing = s)}>Modifier</button>
              <button class="link danger" onclick={() => remove(s)}>Supprimer</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

{#if editing}
  <StudentForm
    student={editing === "new" ? null : editing}
    defaultCohort={cohortId}
    onclose={() => (editing = null)}
    onsaved={load}
  />
{/if}
{#if importing}
  <ImportWizard {cohortId} onclose={() => (importing = false)} ondone={load} />
{/if}

<style>
  .filters { margin-bottom: 1rem; }
  tr.inactive td { opacity: 0.5; }
</style>
