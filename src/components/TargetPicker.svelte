<script lang="ts">
  // Sélection de promotions entières et/ou de groupes, présentée en arbre.
  import type { Id, ProgramNode } from "../lib/types";

  let { tree, groupIds = $bindable([]), cohortIds = $bindable([]) }: {
    tree: ProgramNode[];
    groupIds: Id[];
    cohortIds: Id[];
  } = $props();

  function toggle(list: Id[], id: Id, on: boolean): Id[] {
    return on ? [...new Set([...list, id])] : list.filter((x) => x !== id);
  }
</script>

{#if tree.length === 0}
  <p class="muted small">Aucun groupe. Créez cursus, promotions et groupes dans « Groupes » ou importez une liste.</p>
{:else}
  <div class="tree">
    {#each tree as p (p.id)}
      <div class="prog">{p.name}</div>
      {#each p.cohorts as co (co.id)}
        <div class="cohort">
          <label>
            <input
              type="checkbox"
              checked={cohortIds.includes(co.id)}
              onchange={(e) => (cohortIds = toggle(cohortIds, co.id, e.currentTarget.checked))}
            />
            <strong>{co.name}</strong>
            {#if co.school_year}<span class="muted small">{co.school_year}</span>{/if}
            <span class="muted small">· promo entière ({co.student_count})</span>
          </label>
          <div class="groups">
            {#each co.groups as g (g.id)}
              <label class="chip" class:dim={cohortIds.includes(co.id)}>
                <input
                  type="checkbox"
                  checked={groupIds.includes(g.id)}
                  disabled={cohortIds.includes(co.id)}
                  onchange={(e) => (groupIds = toggle(groupIds, g.id, e.currentTarget.checked))}
                />
                {g.name} <span class="muted small">({g.student_count})</span>
              </label>
            {/each}
          </div>
        </div>
      {/each}
    {/each}
  </div>
{/if}

<style>
  .tree { border: 1px solid var(--border); border-radius: 8px; padding: 0.6rem 0.8rem; max-height: 280px; overflow: auto; }
  .prog { font-weight: 700; margin: 0.3rem 0; }
  .cohort { margin: 0 0 0.6rem 0.6rem; }
  .cohort > label { display: flex; align-items: center; gap: 0.45rem; }
  .groups { display: flex; flex-wrap: wrap; gap: 0.4rem; margin: 0.35rem 0 0 1.5rem; }
  .chip { display: inline-flex; align-items: center; gap: 0.35rem; border: 1px solid var(--border); border-radius: 99px; padding: 0.15rem 0.6rem; }
  .chip.dim { opacity: 0.45; }
</style>
