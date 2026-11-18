<script lang="ts">
  // Test des cartes : tant que la page est affichée, les badges sont diagnostiqués
  // (UID, type, ATR/SAK, étudiant associé) et ne sont jamais enregistrés comme présences.
  import { onMount } from "svelte";
  import { api, events } from "../lib/api";
  import { guard, reader, toast } from "../lib/app.svelte";
  import { fmtTimeSec } from "../lib/format";
  import Modal from "../components/Modal.svelte";
  import StudentPicker from "../components/StudentPicker.svelte";
  import type { CardDiagnostic, Student, StudentRow } from "../lib/types";

  interface Pass extends CardDiagnostic { n: number }

  let history = $state<Pass[]>([]);
  let last = $derived(history[0] ?? null);
  let linking = $state<string | null>(null);
  let mockUid = $state("");
  let n = 0;

  let distinct = $derived(new Set(history.map((p) => p.uid)).size);

  const name = (s: Student) => `${s.last_name.toUpperCase()} ${s.first_name}`;
  const hex = (b: number | null) => (b === null ? "—" : "0x" + b.toString(16).toUpperCase().padStart(2, "0"));

  function verdict(d: CardDiagnostic): { cls: string; text: string } {
    if (d.random_uid) return { cls: "absent", text: "UID aléatoire : inutilisable pour l'émargement" };
    if (d.student) return { cls: "present", text: `Associée à ${name(d.student)}` };
    return { cls: "late", text: "Carte lisible, associée à personne" };
  }

  onMount(() => {
    api.setCardTest(true);
    const off = events.onCardTest((d) => {
      history = [{ ...d, n: ++n }, ...history].slice(0, 50);
    });
    return () => {
      api.setCardTest(false);
      off.then((f) => f());
    };
  });

  async function inject() {
    if ((await guard(api.injectCard(mockUid))) !== undefined) mockUid = "";
  }

  async function copy(uid: string) {
    try {
      await navigator.clipboard.writeText(uid);
      toast("UID copié", "success", 2000);
    } catch {
      toast("Copie impossible", "error");
    }
  }

  async function link(st: StudentRow) {
    const uid = linking;
    if (!uid || (await guard(api.linkCard(uid, st.id))) === undefined) return;
    linking = null;
    history = history.map((p) => (p.uid === uid ? { ...p, student: st } : p));
    toast(`Carte associée à ${name(st)}`, "success");
  }
</script>

<div class="page-head">
  <div>
    <h1>Test cartes</h1>
    <p class="muted small">Posez une carte sur le lecteur pour la diagnostiquer. Sur cette page, aucun badge n'est enregistré, même si un créneau est ouvert.</p>
  </div>
  {#if history.length}
    <button onclick={() => (history = [])}>Effacer l'historique</button>
  {/if}
</div>

<div class="stack">
  <section class="card">
    <h2>Lecteur(s)</h2>
    {#if reader.readers.length}
      <ul class="readers small">
        {#each reader.readers as r (r.name)}
          <li>
            <span class="dot" class:on={r.connected}></span>
            <span class="mono grow">{r.name}</span>
            <span class="muted">{r.connected ? r.message : r.message || "déconnecté"}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="small muted">{reader.message}</p>
    {/if}
    {#if reader.backend === "mock"}
      <form class="row mock" onsubmit={(e) => { e.preventDefault(); inject(); }}>
        <span class="small muted">Lecteur simulé :</span>
        <input bind:value={mockUid} placeholder="UID à tester (ex. 04:24:A5:82:E6:19:90)" size="34" />
        <button type="submit" disabled={!mockUid.trim()}>Badger</button>
      </form>
    {/if}
  </section>

  <section class="card result">
    {#if last}
      {@const v = verdict(last)}
      <div class="verdict {v.cls}">{v.text}</div>
      <div class="uid-line">
        <span class="mono uid">{last.uid}</span>
        <button class="small-btn" onclick={() => copy(last.uid)}>Copier</button>
        {#if !last.random_uid && !last.student}
          <button class="primary small-btn" onclick={() => (linking = last.uid)}>Associer à un étudiant…</button>
        {/if}
      </div>
      <dl>
        <dt>Type probable</dt><dd>{last.card_type}</dd>
        <dt>Longueur UID</dt><dd>{last.uid_len} octets</dd>
        <dt>SAK</dt><dd class="mono">{hex(last.sak)}</dd>
        <dt>ATR</dt><dd class="mono">{last.atr ?? "—"}</dd>
        <dt>Lecteur</dt><dd class="mono">{last.reader}</dd>
        <dt>Lu à</dt><dd>{fmtTimeSec(last.scanned_at)}</dd>
      </dl>
    {:else}
      <div class="empty">
        <div class="big" aria-hidden="true">▭</div>
        En attente d'une carte…
      </div>
    {/if}
  </section>

  {#if history.length}
    <section class="card">
      <h2>Historique <span class="muted small">· {history.length} passage(s), {distinct} carte(s) différente(s)</span></h2>
      <table>
        <thead><tr><th>#</th><th>Heure</th><th>UID</th><th>Type</th><th>Lecteur</th><th>Étudiant</th></tr></thead>
        <tbody>
          {#each history as p (p.n)}
            <tr>
              <td class="muted">{p.n}</td>
              <td>{fmtTimeSec(p.scanned_at)}</td>
              <td class="mono">{p.uid}</td>
              <td class="small">{p.card_type}</td>
              <td class="mono small">{p.reader}</td>
              <td>
                {#if p.random_uid}<span class="pill absent">UID aléatoire</span>
                {:else if p.student}{name(p.student)}
                {:else}<span class="pill late">Inconnue</span>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="muted small">Une même carte reposée moins de 3 s après sur le même lecteur est ignorée (anti-rebond).</p>
    </section>
  {/if}
</div>

{#if linking}
  <Modal title="Associer la carte" onclose={() => (linking = null)}>
    <p>À qui appartient la carte <span class="mono">{linking}</span> ?</p>
    <StudentPicker onpick={link} hideWithCard />
  </Modal>
{/if}

<style>
  .readers { list-style: none; margin: 0.25rem 0 0; padding: 0; display: flex; flex-direction: column; gap: 0.3rem; }
  .readers li { display: flex; align-items: center; gap: 0.5rem; }
  .readers .dot { width: 0.6rem; height: 0.6rem; border-radius: 50%; background: var(--absent); flex: none; }
  .readers .dot.on { background: var(--present); }
  .mock { margin-top: 0.8rem; padding: 0.5rem 0.8rem; border: 1px dashed var(--border); border-radius: 8px; }
  .verdict { display: inline-block; padding: 0.35rem 0.8rem; border-radius: 999px; font-weight: 600; margin-bottom: 0.8rem; }
  .verdict.present { background: var(--present-bg); color: var(--present); }
  .verdict.late { background: var(--late-bg); color: var(--late); }
  .verdict.absent { background: var(--absent-bg); color: var(--absent); }
  .uid-line { display: flex; align-items: center; gap: 0.6rem; flex-wrap: wrap; margin-bottom: 0.8rem; }
  .uid { font-size: 1.6rem; font-weight: 700; letter-spacing: 0.02em; }
  .small-btn { font-size: 0.85rem; padding: 0.25rem 0.6rem; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 0.35rem 1.2rem; margin: 0; }
  dt { color: var(--muted); font-size: 0.88rem; }
  dd { margin: 0; word-break: break-all; }
  .big { font-size: 3rem; line-height: 1; margin-bottom: 0.5rem; opacity: 0.5; }
</style>
