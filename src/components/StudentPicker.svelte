<script lang="ts">
  // Recherche d'un étudiant (nom, prénom ou numéro). `preferIds` remonte en tête (ex. attendus du créneau).
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import type { Id, StudentRow } from "../lib/types";

  let { onpick, preferIds = [], hideWithCard = false }: {
    onpick: (s: StudentRow) => void;
    preferIds?: Id[];
    hideWithCard?: boolean;
  } = $props();

  let all = $state<StudentRow[]>([]);
  let q = $state("");
  let input: HTMLInputElement;

  const norm = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

  let shown = $derived.by(() => {
    const words = norm(q).split(/\s+/).filter(Boolean);
    const prefer = new Set(preferIds);
    return all
      .filter((s) => !hideWithCard || s.cards.length === 0)
      .filter((s) => {
        const hay = norm(`${s.last_name} ${s.first_name} ${s.student_number} ${s.groups}`);
        return words.every((w) => hay.includes(w));
      })
      .sort((a, b) => Number(prefer.has(b.id)) - Number(prefer.has(a.id)))
      .slice(0, 60);
  });

  onMount(async () => {
    all = await api.listStudents({});
    input?.focus();
  });
</script>

<input bind:this={input} bind:value={q} placeholder="Rechercher : nom, prénom, n° étudiant, groupe…" />
<div class="list">
  {#each shown as s (s.id)}
    <button class="item" onclick={() => onpick(s)}>
      <span><strong>{s.last_name.toUpperCase()}</strong> {s.first_name}</span>
      <span class="muted small">
        {s.student_number}{s.groups ? " · " + s.groups : ""}{s.cards.length ? " · carte ✓" : ""}
        {#if preferIds.includes(s.id)}<span class="pill planned">attendu</span>{/if}
      </span>
    </button>
  {:else}
    <p class="muted small">Aucun étudiant trouvé.</p>
  {/each}
</div>

<style>
  .list { display: flex; flex-direction: column; max-height: 340px; overflow: auto; border: 1px solid var(--border); border-radius: 8px; }
  .item { display: flex; justify-content: space-between; gap: 1rem; border: none; border-bottom: 1px solid var(--border); border-radius: 0; text-align: left; padding: 0.55rem 0.75rem; }
  .item:last-child { border-bottom: none; }
</style>
