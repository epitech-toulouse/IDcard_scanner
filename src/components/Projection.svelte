<script lang="ts">
  // Écran plein cadre lisible de loin : créneau, dernier badge, compteur, derniers passages.
  import type { Snippet } from "svelte";
  import { fmtDate, fmtTime } from "../lib/format";
  import type { ScanMessage } from "../lib/scan";
  import type { SessionRow } from "../lib/types";

  let {
    session,
    here,
    flash,
    recent = [],
    onexit,
    exitLabel = "Quitter la projection",
    badge,
  }: {
    session: SessionRow;
    here: number;
    flash: ScanMessage | null;
    recent?: { id: number; cls: string; title: string }[];
    onexit: () => void;
    exitLabel?: string;
    badge?: Snippet;
  } = $props();

  let pct = $derived(session.counts.expected ? Math.min(100, (100 * here) / session.counts.expected) : 0);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onexit()} />

<div class="projection">
  <div class="top">
    {#if badge}{@render badge()}{/if}
    <button class="exit" onclick={onexit}>{exitLabel}</button>
  </div>
  <h1>{session.title}</h1>
  <p>{fmtDate(session.starts_at)} · {fmtTime(session.starts_at)} – {fmtTime(session.ends_at)}{session.room ? ` · ${session.room}` : ""}</p>
  <div class="pbanner {flash?.cls ?? 'idle'}">
    {#if flash}<div class="pwho">{flash.title}</div><div class="psub">{flash.sub}</div>
    {:else if session.state === "open"}<div class="pwho">Badgez votre carte étudiant</div>
    {:else}<div class="pwho muted">Émargement {session.state === "closed" ? "clos" : "non ouvert"}</div>{/if}
  </div>
  <div class="pcount"><strong>{here}</strong> / {session.counts.expected} présents</div>
  <div class="pbar"><span style:width="{pct}%"></span></div>
  {#if recent.length}
    <div class="recent">
      {#each recent as r (r.id)}<span class="chip {r.cls}">{r.title}</span>{/each}
    </div>
  {/if}
</div>

<style>
  .projection { position: fixed; inset: 0; z-index: 40; background: var(--bg); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 1.2rem; text-align: center; padding: 2rem; }
  .projection h1 { font-size: 2.6rem; margin: 0; }
  .projection p { font-size: 1.3rem; color: var(--muted); margin: 0; }
  .top { position: absolute; top: 1rem; right: 1rem; left: 1rem; display: flex; justify-content: flex-end; align-items: center; gap: 0.8rem; }
  .pbanner { width: min(1000px, 92vw); min-height: 260px; border-radius: 20px; display: flex; flex-direction: column; justify-content: center; border: 3px solid var(--border); background: var(--surface); transition: background 0.2s; }
  .pbanner.present { border-color: var(--present); background: var(--present-bg); }
  .pbanner.late { border-color: var(--late); background: var(--late-bg); }
  .pbanner.absent { border-color: var(--absent); background: var(--absent-bg); }
  .pbanner.excused { border-color: var(--excused); background: var(--excused-bg); }
  .pwho { font-size: 4rem; font-weight: 800; }
  .psub { font-size: 2rem; }
  .pcount { font-size: 2rem; }
  .pcount strong { font-size: 3rem; font-variant-numeric: tabular-nums; }
  .pbar { width: min(600px, 80vw); height: 12px; border-radius: 99px; background: var(--none-bg); overflow: hidden; }
  .pbar span { display: block; height: 100%; background: var(--present); transition: width 0.3s; }
  .recent { display: flex; flex-wrap: wrap; justify-content: center; gap: 0.5rem; max-width: min(1000px, 92vw); }
  .chip { padding: 0.35rem 0.8rem; border-radius: 99px; font-size: 1.1rem; border: 2px solid var(--border); background: var(--surface); }
  .chip.present { border-color: var(--present); }
  .chip.late { border-color: var(--late); }
</style>
