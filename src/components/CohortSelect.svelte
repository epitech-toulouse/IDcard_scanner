<script lang="ts">
  import type { Id, ProgramNode } from "../lib/types";

  let { tree, value = $bindable(null), allowNone = false, noneLabel = "— Aucune —" }: {
    tree: ProgramNode[];
    value: Id | null;
    allowNone?: boolean;
    noneLabel?: string;
  } = $props();
</script>

<select
  value={value ?? ""}
  onchange={(e) => (value = e.currentTarget.value === "" ? null : Number(e.currentTarget.value))}
>
  {#if allowNone || value === null}<option value="">{noneLabel}</option>{/if}
  {#each tree as p (p.id)}
    <optgroup label={p.name}>
      {#each p.cohorts as co (co.id)}
        <option value={co.id}>{co.name}{co.school_year ? ` (${co.school_year})` : ""}</option>
      {/each}
    </optgroup>
  {/each}
</select>
