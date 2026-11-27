<script lang="ts">
  // Onglets communs aux pages Étudiants et Groupes (une seule entrée dans la navigation).
  let { current }: { current: "etudiants" | "groupes" } = $props();
</script>

<div class="tabs people" role="tablist" aria-label="Étudiants et groupes">
  <a href="#/etudiants" role="tab" aria-selected={current === "etudiants"} class:on={current === "etudiants"}>Étudiants</a>
  <a href="#/groupes" role="tab" aria-selected={current === "groupes"} class:on={current === "groupes"}>Cursus, promotions et groupes</a>
</div>

<style>
  .people { margin-bottom: 1.2rem; }
</style>
