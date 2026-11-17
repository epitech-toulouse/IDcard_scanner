<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { confirmAction, guard, navigate, toast } from "../lib/app.svelte";
  import { schoolYear } from "../lib/format";
  import type { Id, NodeKind, ProgramNode } from "../lib/types";

  let tree = $state<ProgramNode[]>([]);
  let newProgram = $state("");
  let newCohort = $state<Record<Id, { name: string; year: string }>>({});
  let newGroup = $state<Record<Id, string>>({});
  let editing = $state<{ kind: NodeKind; id: Id; name: string; year?: string } | null>(null);

  async function load() {
    tree = (await guard(api.tree())) ?? [];
    for (const p of tree) newCohort[p.id] ??= { name: "", year: schoolYear() };
  }
  onMount(load);

  async function addProgram() {
    if (await guard(api.createProgram(newProgram))) {
      newProgram = "";
      await load();
    }
  }

  async function addCohort(programId: Id) {
    const f = newCohort[programId] ?? { name: "", year: schoolYear() };
    if (await guard(api.createCohort(programId, f.name, f.year || schoolYear()))) {
      newCohort[programId] = { name: "", year: f.year };
      await load();
    }
  }

  async function addGroup(cohortId: Id) {
    if (await guard(api.createGroup(cohortId, newGroup[cohortId] ?? ""))) {
      newGroup[cohortId] = "";
      await load();
    }
  }

  async function saveEdit() {
    if (!editing) return;
    const e = editing;
    const ok = await guard(
      (async () => {
        await api.renameNode(e.kind, e.id, e.name);
        if (e.kind === "cohort" && e.year !== undefined) await api.setSchoolYear(e.id, e.year);
      })(),
    );
    if (ok !== undefined) {
      editing = null;
      await load();
    }
  }

  const WHAT: Record<NodeKind, string> = {
    program: "ce cursus, ses promotions, ses groupes et les créneaux qui les ciblent",
    cohort: "cette promotion et ses groupes (les étudiants resteront, sans promotion)",
    group: "ce groupe (les étudiants restent dans la promotion)",
  };

  async function remove(kind: NodeKind, id: Id, name: string) {
    if (!(await confirmAction(`Supprimer « ${name} » ?\n\nCela supprime ${WHAT[kind]}.`))) return;
    await guard(api.deleteNode(kind, id));
    toast(`« ${name} » supprimé`, "success");
    await load();
  }
</script>

<div class="page-head">
  <div>
    <h1>Groupes</h1>
    <p class="muted">Cursus → promotion → groupes de TD/TP. Un étudiant appartient à une promotion et à un ou plusieurs groupes.</p>
  </div>
  <form class="row" onsubmit={(e) => { e.preventDefault(); addProgram(); }}>
    <input bind:value={newProgram} placeholder="Nouveau cursus (ex. BUT Informatique)" size="32" />
    <button class="primary" disabled={!newProgram.trim()}>Ajouter le cursus</button>
  </form>
</div>

{#if tree.length === 0}
  <div class="card empty">
    <p>Aucun cursus pour l'instant.</p>
    <p class="small">Créez-en un ci-dessus, ou <a href="#/etudiants" onclick={(e) => { e.preventDefault(); navigate("etudiants"); }}>importez une liste d'étudiants</a> (les groupes seront créés automatiquement).</p>
  </div>
{/if}

<div class="stack">
  {#each tree as p (p.id)}
    <section class="card">
      <div class="head">
        {#if editing?.kind === "program" && editing.id === p.id}
          <form class="row" onsubmit={(e) => { e.preventDefault(); saveEdit(); }}>
            <input bind:value={editing.name} />
            <button class="primary">Enregistrer</button>
            <button type="button" onclick={() => (editing = null)}>Annuler</button>
          </form>
        {:else}
          <h2>{p.name}</h2>
          <div>
            <button class="link" onclick={() => (editing = { kind: "program", id: p.id, name: p.name })}>Renommer</button>
            <button class="link danger" onclick={() => remove("program", p.id, p.name)}>Supprimer</button>
          </div>
        {/if}
      </div>

      <div class="cohorts">
        {#each p.cohorts as co (co.id)}
          <div class="cohort">
            <div class="head">
              {#if editing?.kind === "cohort" && editing.id === co.id}
                <form class="row" onsubmit={(e) => { e.preventDefault(); saveEdit(); }}>
                  <input bind:value={editing.name} size="14" />
                  <input bind:value={editing.year} size="10" placeholder="2026-2027" />
                  <button class="primary">OK</button>
                  <button type="button" onclick={() => (editing = null)}>Annuler</button>
                </form>
              {:else}
                <div>
                  <strong>{co.name}</strong>
                  <span class="muted small">{co.school_year} · {co.student_count} étudiant{co.student_count > 1 ? "s" : ""}</span>
                </div>
                <div>
                  <button class="link" onclick={() => navigate(`etudiants/${co.id}`)}>Étudiants</button>
                  <button class="link" onclick={() => (editing = { kind: "cohort", id: co.id, name: co.name, year: co.school_year })}>Modifier</button>
                  <button class="link danger" onclick={() => remove("cohort", co.id, co.name)}>Supprimer</button>
                </div>
              {/if}
            </div>
            <div class="groups">
              {#each co.groups as g (g.id)}
                {#if editing?.kind === "group" && editing.id === g.id}
                  <form class="row" onsubmit={(e) => { e.preventDefault(); saveEdit(); }}>
                    <input bind:value={editing.name} size="8" />
                    <button class="primary icon">OK</button>
                    <button type="button" class="icon" onclick={() => (editing = null)}>✕</button>
                  </form>
                {:else}
                  <span class="group">
                    {g.name} <span class="muted small">{g.student_count}</span>
                    <button class="link icon" title="Renommer" onclick={() => (editing = { kind: "group", id: g.id, name: g.name })}>✎</button>
                    <button class="link icon danger" title="Supprimer" onclick={() => remove("group", g.id, g.name)}>✕</button>
                  </span>
                {/if}
              {/each}
              <form class="row" onsubmit={(e) => { e.preventDefault(); addGroup(co.id); }}>
                <input bind:value={newGroup[co.id]} placeholder="Nouveau groupe" size="12" />
                <button disabled={!newGroup[co.id]?.trim()}>+</button>
              </form>
            </div>
          </div>
        {/each}

        {#if newCohort[p.id]}
          <form class="row add-cohort" onsubmit={(e) => { e.preventDefault(); addCohort(p.id); }}>
            <input bind:value={newCohort[p.id].name} placeholder="Nouvelle promotion (ex. BUT2)" size="22" />
            <input bind:value={newCohort[p.id].year} placeholder="Année" size="10" />
            <button disabled={!newCohort[p.id].name.trim()}>Ajouter la promotion</button>
          </form>
        {/if}
      </div>
    </section>
  {/each}
</div>

<style>
  .head { display: flex; justify-content: space-between; align-items: center; gap: 1rem; }
  .head h2 { margin: 0; }
  .cohorts { margin-top: 0.8rem; display: flex; flex-direction: column; gap: 0.7rem; }
  .cohort { border: 1px solid var(--border); border-radius: 8px; padding: 0.7rem 0.9rem; background: var(--surface-2); }
  .groups { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; margin-top: 0.5rem; }
  .group { display: inline-flex; align-items: center; gap: 0.3rem; background: var(--surface); border: 1px solid var(--border); border-radius: 99px; padding: 0.1rem 0.25rem 0.1rem 0.75rem; }
  .add-cohort { margin-top: 0.2rem; }
</style>
