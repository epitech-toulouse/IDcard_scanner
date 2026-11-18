<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { EXPORT_FILTERS, guard, saveFile, toast, type ExportFormat } from "../lib/app.svelte";
  import { joinNaive, safeFileName, schoolYear, toNaive } from "../lib/format";
  import TargetPicker from "../components/TargetPicker.svelte";
  import type { Id, ProgramNode } from "../lib/types";

  let tree = $state<ProgramNode[]>([]);
  const year = Number(schoolYear().slice(0, 4));
  let from = $state(`${year}-09-01`);
  let to = $state(toNaive(new Date()).slice(0, 10));
  let groupIds = $state<Id[]>([]);
  let cohortIds = $state<Id[]>([]);
  let busy = $state(false);

  onMount(async () => (tree = (await guard(api.tree())) ?? []));

  function preset(kind: "week" | "month" | "year") {
    const now = new Date();
    const d = new Date(now);
    if (kind === "week") d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
    if (kind === "month") d.setDate(1);
    from = kind === "year" ? `${year}-09-01` : toNaive(d).slice(0, 10);
    to = toNaive(now).slice(0, 10);
  }

  function targetName(): string {
    const names: string[] = [];
    for (const p of tree)
      for (const c of p.cohorts) {
        if (cohortIds.includes(c.id)) names.push(c.name);
        for (const g of c.groups) if (groupIds.includes(g.id)) names.push(`${c.name}-${g.name}`);
      }
    return names.length ? names.slice(0, 3).join("_") : "tous";
  }

  async function run(format: ExportFormat) {
    const path = await saveFile(safeFileName(`Presences_${targetName()}_${from}_${to}`) + "." + format, EXPORT_FILTERS[format]);
    if (!path) return;
    busy = true;
    const r = await guard(
      api.exportPeriod(
        { from: from ? joinNaive(from, "00:00") : null, to: to ? joinNaive(to, "23:59:59") : null, group_ids: groupIds, cohort_ids: cohortIds },
        path,
      ),
    );
    busy = false;
    if (r) toast(`Export : ${r.sessions} créneau(x), ${r.students} étudiant(s) → ${path}`, "success", 7000);
  }
</script>

<div class="page-head">
  <div>
    <h1>Exports</h1>
    <p class="muted">Récapitulatif des présences sur une période : une feuille « Présences » (étudiants × créneaux) et une feuille « Statistiques » (taux par étudiant et par groupe).</p>
  </div>
</div>

<div class="card stack">
  <div class="row">
    <label class="field">Du <input type="date" bind:value={from} /></label>
    <label class="field">Au <input type="date" bind:value={to} /></label>
    <div class="row presets">
      <button onclick={() => preset("week")}>Cette semaine</button>
      <button onclick={() => preset("month")}>Ce mois</button>
      <button onclick={() => preset("year")}>Année scolaire</button>
    </div>
  </div>
  <div class="field">
    <span class="muted small">Étudiants concernés (rien de coché = tous)</span>
    <TargetPicker {tree} bind:groupIds bind:cohortIds />
  </div>
  <div class="row">
    <button class="primary big" disabled={busy} onclick={() => run("xlsx")}>{busy ? "Export…" : "Exporter en Excel…"}</button>
    <button class="big" disabled={busy} onclick={() => run("csv")}>Exporter en CSV…</button>
  </div>
  <p class="muted small">
    Codes : P présent · R retard · AJ absence justifiée · A absent · « · » créneau pas encore clos. Le taux de présence compte présents + retards
    sur les créneaux où l'étudiant était attendu. Le CSV (séparateur « ; », s'ouvre dans Excel ou LibreOffice) contient la matrice
    et le bilan par étudiant, sans les statistiques par groupe. Pour la feuille d'un seul créneau, utilisez « Excel » ou « CSV » dans la liste des créneaux.
  </p>
</div>

<style>
  .presets { align-self: flex-end; }
</style>
