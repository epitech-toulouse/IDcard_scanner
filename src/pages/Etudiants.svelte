<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { api } from "../lib/api";
  import { confirmAction, guard, route, toast } from "../lib/app.svelte";
  import CohortSelect from "../components/CohortSelect.svelte";
  import StudentForm from "../components/StudentForm.svelte";
  import ImportWizard from "../components/ImportWizard.svelte";
  import type { BulkAction, Id, ProgramNode, StudentRow } from "../lib/types";

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

  // ---------- Sélection et actions groupées ----------
  type BulkKind = BulkAction["kind"] | "set_inactive" | "";
  const selected = new SvelteSet<Id>();
  let lastClicked: number | null = null;
  let bulkKind = $state<BulkKind>("");
  let bulkCohort = $state<Id | null>(null);
  let bulkGroup = $state<Id | null>(null);
  let busy = $state(false);

  let selectedRows = $derived(rows.filter((r) => selected.has(r.id)));
  let allSelected = $derived(rows.length > 0 && selectedRows.length === rows.length);
  // Groupes proposés : tous pour l'ajout, seulement ceux de la sélection pour le retrait.
  let allGroups = $derived(
    tree.flatMap((p) => p.cohorts.flatMap((co) => co.groups.map((g) => ({ id: g.id, label: `${co.name} / ${g.name}` })))),
  );
  let bulkGroups = $derived(
    bulkKind === "remove_from_group"
      ? allGroups.filter((g) => selectedRows.some((r) => r.group_ids.includes(g.id)))
      : allGroups,
  );
  let needsGroup = $derived(bulkKind === "add_to_group" || bulkKind === "remove_from_group");
  let canApply = $derived(bulkKind !== "" && (!needsGroup || bulkGroup !== null) && !busy);

  function toggle(index: number, e: MouseEvent) {
    const id = rows[index].id;
    const on = !selected.has(id);
    // Maj+clic : applique le même état à toute la plage depuis le dernier clic.
    const from = e.shiftKey && lastClicked !== null ? Math.min(lastClicked, index) : index;
    const to = e.shiftKey && lastClicked !== null ? Math.max(lastClicked, index) : index;
    for (let i = from; i <= to; i++) {
      if (on) selected.add(rows[i].id);
      else selected.delete(rows[i].id);
    }
    lastClicked = index;
  }

  function toggleAll() {
    if (allSelected) selected.clear();
    else rows.forEach((r) => selected.add(r.id));
  }

  function buildAction(): BulkAction | null {
    switch (bulkKind) {
      case "set_active": return { kind: "set_active", active: true };
      case "set_inactive": return { kind: "set_active", active: false };
      case "unlink_cards": return { kind: "unlink_cards" };
      case "delete": return { kind: "delete" };
      case "set_cohort": return { kind: "set_cohort", cohort_id: bulkCohort };
      case "add_to_group": return bulkGroup === null ? null : { kind: "add_to_group", group_id: bulkGroup };
      case "remove_from_group": return bulkGroup === null ? null : { kind: "remove_from_group", group_id: bulkGroup };
      default: return null;
    }
  }

  async function applyBulk(action: BulkAction | null = buildAction()) {
    const ids = selectedRows.map((r) => r.id);
    if (!action || !ids.length) return;
    const n = ids.length;
    if (action.kind === "delete") {
      const ok = await confirmAction(
        `Supprimer ${n} étudiant(s) et tout leur historique de présence ?\n\nPour des étudiants partis, préférez les passer « inactifs ».`,
      );
      if (!ok) return;
    } else if (action.kind === "unlink_cards") {
      const cards = selectedRows.reduce((t, r) => t + r.cards.length, 0);
      if (!(await confirmAction(`Dissocier ${cards} carte(s) de ${n} étudiant(s) ?`))) return;
    }
    busy = true;
    const done = await guard(api.bulkStudents(ids, action));
    busy = false;
    if (done === undefined) return;
    const verbs: Record<BulkAction["kind"], string> = {
      set_active: action.kind === "set_active" && !action.active ? "désactivé(s)" : "activé(s)",
      set_cohort: "déplacé(s)",
      add_to_group: "ajouté(s) au groupe",
      remove_from_group: "retiré(s) du groupe",
      unlink_cards: "sans carte désormais",
      delete: "supprimé(s)",
    };
    toast(`${done} étudiant(s) ${verbs[action.kind]}`, "success");
    if (action.kind === "delete") selected.clear();
    bulkKind = "";
    bulkGroup = null;
    load();
  }

  async function load() {
    tree = (await guard(api.tree())) ?? [];
    if (groupId !== null && !groups.some((g) => g.id === groupId)) groupId = null;
    rows =
      (await guard(
        api.listStudents({ cohort_id: cohortId, group_id: groupId, search: search || null, include_inactive: includeInactive }),
      )) ?? [];
    // La sélection ne garde que les étudiants encore affichés.
    const visible = new Set(rows.map((r) => r.id));
    for (const id of [...selected]) if (!visible.has(id)) selected.delete(id);
    lastClicked = null;
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

{#if selectedRows.length}
  <div class="card bulk row">
    <strong>{selectedRows.length} sélectionné(s)</strong>
    <select bind:value={bulkKind} onchange={() => (bulkGroup = null)} aria-label="Action groupée">
      <option value="">Choisir une action…</option>
      <optgroup label="Statut">
        <option value="set_active">Activer</option>
        <option value="set_inactive">Passer inactifs</option>
      </optgroup>
      <optgroup label="Affectation">
        <option value="set_cohort">Changer de promotion…</option>
        <option value="add_to_group">Ajouter à un groupe…</option>
        <option value="remove_from_group">Retirer d'un groupe…</option>
      </optgroup>
      <optgroup label="Cartes et suppression">
        <option value="unlink_cards">Dissocier les cartes</option>
        <option value="delete">Supprimer</option>
      </optgroup>
    </select>
    {#if bulkKind === "set_cohort"}
      <CohortSelect {tree} bind:value={bulkCohort} allowNone noneLabel="Sans promotion" />
    {:else if needsGroup}
      <select
        value={bulkGroup ?? ""}
        onchange={(e) => (bulkGroup = e.currentTarget.value === "" ? null : Number(e.currentTarget.value))}
        aria-label="Groupe"
      >
        <option value="">{bulkGroups.length ? "Choisir un groupe…" : "Aucun groupe"}</option>
        {#each bulkGroups as g (g.id)}<option value={g.id}>{g.label}</option>{/each}
      </select>
    {/if}
    <button class:primary={bulkKind !== "delete"} class:danger={bulkKind === "delete"} disabled={!canApply} onclick={() => applyBulk()}>
      Appliquer
    </button>
    <span class="grow"></span>
    <button class="link" onclick={() => selected.clear()}>Désélectionner</button>
  </div>
{/if}

{#if rows.length === 0}
  <div class="card empty">
    <p>Aucun étudiant{search || cohortId ? " pour ce filtre" : ""}.</p>
    <p class="small">Importez la liste exportée de la scolarité (CSV / Excel), puis associez les cartes au premier badge.</p>
  </div>
{:else}
  <div class="table-wrap">
    <table>
      <thead>
        <tr>
          <th class="check">
            <input
              type="checkbox"
              checked={allSelected}
              indeterminate={selectedRows.length > 0 && !allSelected}
              onchange={toggleAll}
              title="Tout sélectionner"
              aria-label="Tout sélectionner"
            />
          </th>
          <th>Nom</th><th>Prénom</th><th>N° étudiant</th><th>Promotion</th><th>Groupes</th><th>Carte</th><th></th>
        </tr>
      </thead>
      <tbody>
        {#each rows as s, i (s.id)}
          <tr class:inactive={!s.active} class:selected={selected.has(s.id)}>
            <td class="check">
              <input
                type="checkbox"
                checked={selected.has(s.id)}
                onclick={(e) => toggle(i, e)}
                title="Maj+clic pour sélectionner une plage"
                aria-label={`Sélectionner ${s.last_name} ${s.first_name}`}
              />
            </td>
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
  .bulk { margin-bottom: 1rem; padding: 0.6rem 0.9rem; border-color: var(--accent); }
  th.check, td.check { width: 2rem; padding-right: 0; }
  tr.inactive td:not(.check) { opacity: 0.5; }
  tr.selected { background: var(--excused-bg); }
  tr.selected:hover { background: var(--excused-bg); filter: brightness(0.98); }
</style>
