<script lang="ts">
  import { onMount } from "svelte";
  import { api, events } from "../lib/api";
  import { confirmAction, guard, navigate, reader, toast } from "../lib/app.svelte";
  import { exportSession } from "../lib/actions";
  import { fmtRange, fmtTime, fmtTimeSec, isToday, toNaive } from "../lib/format";
  import Modal from "../components/Modal.svelte";
  import StatusPill from "../components/StatusPill.svelte";
  import StudentPicker from "../components/StudentPicker.svelte";
  import Projection from "../components/Projection.svelte";
  import { describeScan, studentName as name } from "../lib/scan";
  import {
    SOURCE_LABEL, STATE_LABEL, STATUS_LABEL,
    type AttendanceRow, type Id, type ScanOutcome, type SessionDetail, type SessionRow, type Status, type Student,
  } from "../lib/types";

  let { sessionId }: { sessionId: Id | null } = $props();

  let detail = $state<SessionDetail | null>(null);
  let candidates = $state<SessionRow[]>([]);
  let feed = $state<{ id: number; cls: string; title: string; sub: string; at: string }[]>([]);
  let flash = $state<{ cls: string; title: string; sub: string } | null>(null);
  let unknownUid = $state<string | null>(null);
  let notExpected = $state<Student | null>(null);
  let excusing = $state<AttendanceRow | null>(null);
  let excuseReason = $state("");
  let onlyMissing = $state(false);
  let projection = $state(false);
  let mockUid = $state("");
  let feedId = 0;
  let flashTimer: ReturnType<typeof setTimeout>;

  let s = $derived(detail?.session ?? null);
  let expectedRows = $derived(detail?.rows.filter((r) => r.expected) ?? []);
  let here = $derived(detail?.rows.filter((r) => r.status === "present" || r.status === "late").length ?? 0);
  let missing = $derived(expectedRows.filter((r) => !r.status).length);
  let barPct = $derived(s && s.counts.expected ? Math.min(100, (100 * here) / s.counts.expected) : 0);
  let recent = $derived(feed.filter((f, i) => (f.cls === "present" || f.cls === "late") && feed.findIndex((g) => g.title === f.title) === i).slice(0, 8));
  let shownRows = $derived(onlyMissing ? (detail?.rows.filter((r) => !r.status) ?? []) : (detail?.rows ?? []));

  async function load() {
    if (sessionId === null) return;
    detail = (await guard(api.sessionDetail(sessionId))) ?? null;
  }

  onMount(() => {
    (async () => {
      if (sessionId === null) {
        const active = await api.getActiveSession();
        if (active !== null) return navigate(`emargement/${active}`);
        const today = new Date();
        const list = (await guard(api.listSessions())) ?? [];
        candidates = list.filter((r) => r.state === "open" || (isToday(r.starts_at, today) && r.state !== "closed")).reverse();
        return;
      }
      await load();
      // Un créneau ouvert affiché ici devient le créneau actif.
      if (detail?.session.state === "open") await api.setActiveSession(sessionId);
    })();
    const off = events.onCard(onCard);
    const offAuto = events.onSessionAuto((e) => {
      if (e.session_id === sessionId) load();
    });
    return () => {
      off.then((f) => f());
      offAuto.then((f) => f());
      clearTimeout(flashTimer);
    };
  });

  function push(cls: string, title: string, sub: string) {
    flash = { cls, title, sub };
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = null), 4000);
    feed = [{ id: feedId++, cls, title, sub, at: fmtTimeSec(toNaive(new Date())) }, ...feed].slice(0, 12);
  }

  async function onCard(o: ScanOutcome) {
    if (sessionId === null) return;
    const m = describeScan(o);
    push(m.cls, m.title, m.sub);
    if (o.kind === "not_expected") notExpected = o.student;
    if (o.kind === "unknown_card") unknownUid = o.uid;
    await load();
  }

  async function openIt() {
    if (sessionId === null) return;
    if ((await guard(s?.state === "closed" ? api.reopenSession(sessionId) : api.openSession(sessionId))) !== undefined) {
      toast("Émargement ouvert : les étudiants peuvent badger", "success");
      await load();
    }
  }

  async function closeIt() {
    if (sessionId === null) return;
    const msg = missing
      ? `${missing} étudiant(s) attendu(s) n'ont pas badgé et seront marqués absents.`
      : "Tous les attendus ont un statut.";
    if (!(await confirmAction(`Clore l'émargement ?\n\n${msg}\nVous pourrez encore corriger ou rouvrir.`))) return;
    const n = await guard(api.closeSession(sessionId));
    if (n !== undefined) {
      toast(`Créneau clos · ${n} absent(s)`, "success");
      projection = false;
      await load();
    }
  }

  async function setStatus(r: AttendanceRow, value: string) {
    if (sessionId === null) return;
    if (value === "excused") {
      excusing = r;
      excuseReason = r.reason ?? "";
      return;
    }
    const p = value === "clear" ? api.clearAttendance(sessionId, r.student.id) : api.setAttendance(sessionId, r.student.id, value as Status);
    if ((await guard(p)) !== undefined) await load();
  }

  async function saveExcuse() {
    if (sessionId === null || !excusing) return;
    if ((await guard(api.setAttendance(sessionId, excusing.student.id, "excused", excuseReason))) !== undefined) {
      excusing = null;
      await load();
    }
  }

  async function linkUnknown(st: Student) {
    if (!unknownUid) return;
    const uid = unknownUid;
    if ((await guard(api.linkCard(uid, st.id))) === undefined) return;
    unknownUid = null;
    toast(`Carte associée à ${name(st)}`, "success");
    await guard(api.rescan(uid)); // le résultat arrive par l'événement card-scanned
  }

  async function acceptUnexpected() {
    if (sessionId === null || !notExpected) return;
    const st = notExpected;
    const status = await guard(api.acceptUnexpected(sessionId, st.id));
    if (status) {
      notExpected = null;
      push(status, name(st), `Ajouté · ${STATUS_LABEL[status].toLowerCase()}`);
      await load();
    }
  }

  async function inject() {
    if ((await guard(api.injectCard(mockUid))) !== undefined) mockUid = "";
  }
</script>

{#if sessionId === null}
  <div class="page-head"><div><h1>Émargement</h1><p class="muted">Choisissez le créneau à émarger.</p></div></div>
  {#if candidates.length}
    <div class="stack">
      {#each candidates as c (c.id)}
        <button class="card pick" onclick={() => navigate(`emargement/${c.id}`)}>
          <span><strong>{c.title}</strong>{c.room ? ` · ${c.room}` : ""}<br /><span class="muted small">{fmtRange(c.starts_at, c.ends_at)} · {c.targets}</span></span>
          <span class="pill {c.state}">{STATE_LABEL[c.state]}</span>
        </button>
      {/each}
    </div>
  {:else}
    <div class="card empty">
      <p>Aucun créneau aujourd'hui.</p>
      <button class="primary" onclick={() => navigate("creneaux")}>Aller aux créneaux</button>
    </div>
  {/if}
{:else if s}
  <div class="page-head">
    <div>
      <h1>{s.title}</h1>
      <p class="muted">{fmtRange(s.starts_at, s.ends_at)}{s.room ? ` · Salle ${s.room}` : ""} · retard après {s.late_after_min} min</p>
      <p class="muted small">{s.targets}</p>
    </div>
    <div class="row">
      <span class="pill {s.state}">{STATE_LABEL[s.state]}</span>
      {#if s.state === "open"}
        <button onclick={() => (projection = true)}>Mode projection</button>
        <button onclick={() => navigate("affichage")} title="Plein écran qui suit tout seul les créneaux ouverts et clos">Affichage automatique</button>
        <button class="primary" onclick={closeIt}>Clore l'émargement</button>
      {:else}
        <button class="primary" onclick={openIt}>{s.state === "closed" ? "Rouvrir" : "Ouvrir l'émargement"}</button>
      {/if}
      <button onclick={() => exportSession(s, "xlsx")}>Export Excel</button>
      <button onclick={() => exportSession(s, "csv")}>Export CSV</button>
    </div>
  </div>

  <div class="live">
    <div class="counters card">
      <div><span class="big">{here}</span><span class="muted">/ {s.counts.expected} présents</span></div>
      <div class="bar"><span style:width="{barPct}%"></span></div>
      <div class="row small">
        <span class="pill late">{s.counts.late} retard(s)</span>
        <span class="pill">{missing} non badgé(s)</span>
        {#if s.counts.excused}<span class="pill excused">{s.counts.excused} justifié(s)</span>{/if}
        {#if s.counts.absent}<span class="pill absent">{s.counts.absent} absent(s)</span>{/if}
      </div>
    </div>

    <div class="banner card {flash?.cls ?? 'idle'}">
      {#if flash}
        <div class="who">{flash.title}</div>
        <div>{flash.sub}</div>
      {:else if s.state === "open"}
        <div class="who">Badgez votre carte étudiant</div>
        <div class="muted">{reader.connected ? reader.message : "⚠ Lecteur non connecté"}</div>
      {:else}
        <div class="who muted">Émargement {s.state === "closed" ? "clos" : "non ouvert"}</div>
      {/if}
    </div>
  </div>

  {#if reader.backend === "mock"}
    <form class="row mock" onsubmit={(e) => { e.preventDefault(); inject(); }}>
      <span class="small muted">Lecteur simulé :</span>
      <input bind:value={mockUid} placeholder="UID à badger (ex. 04:24:A5:82:E6:19:90)" size="34" />
      <button>Badger</button>
    </form>
  {/if}

  <div class="cols">
    <div>
      <div class="row tools">
        <h2 class="grow">Liste ({detail?.rows.length})</h2>
        <label class="row small"><input type="checkbox" bind:checked={onlyMissing} /> non badgés seulement</label>
      </div>
      <div class="table-wrap">
        <table>
          <thead><tr><th>Étudiant</th><th>Groupe(s)</th><th>Statut</th><th>Heure</th><th>Source</th><th></th></tr></thead>
          <tbody>
            {#each shownRows as r (r.student.id)}
              <tr>
                <td>
                  <strong>{r.student.last_name.toUpperCase()}</strong> {r.student.first_name}
                  {#if !r.expected}<span class="pill planned small">non attendu</span>{/if}
                </td>
                <td class="small">{r.groups}</td>
                <td><StatusPill status={r.status} />{#if r.reason}<div class="small muted">{r.reason}</div>{/if}</td>
                <td class="small">{fmtTimeSec(r.scanned_at)}</td>
                <td class="small muted">{r.source ? SOURCE_LABEL[r.source] : ""}</td>
                <td class="actions">
                  <select class="small" value="" onchange={(e) => { setStatus(r, e.currentTarget.value); e.currentTarget.value = ""; }}>
                    <option value="" disabled>Corriger…</option>
                    <option value="present">Présent</option>
                    <option value="late">En retard</option>
                    <option value="excused">Absence justifiée…</option>
                    <option value="absent">Absent</option>
                    <option value="clear">Effacer</option>
                  </select>
                </td>
              </tr>
            {:else}
              <tr><td colspan="6" class="empty">{onlyMissing ? "Tout le monde a un statut 🎉" : "Aucun étudiant attendu : vérifiez les groupes du créneau."}</td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
    <aside>
      <h2>Derniers passages</h2>
      {#each feed as f (f.id)}
        <div class="feed {f.cls}"><span class="small muted">{f.at}</span> <strong>{f.title}</strong><div class="small">{f.sub}</div></div>
      {:else}
        <p class="muted small">Les badges apparaîtront ici.</p>
      {/each}
    </aside>
  </div>

  {#if projection}
    <Projection session={s} {here} {flash} {recent} onexit={() => (projection = false)} />
  {/if}

  {#if unknownUid}
    <Modal title="Carte inconnue" onclose={() => (unknownUid = null)}>
      <p>La carte <span class="mono">{unknownUid}</span> n'est associée à personne. À qui appartient-elle ?</p>
      <StudentPicker onpick={linkUnknown} preferIds={expectedRows.filter((r) => !r.status).map((r) => r.student.id)} />
    </Modal>
  {/if}

  {#if notExpected}
    <Modal title="Étudiant non attendu" onclose={() => (notExpected = null)}>
      <p><strong>{name(notExpected)}</strong> n'appartient à aucun groupe ciblé par ce créneau.</p>
      {#snippet footer()}
        <button onclick={() => (notExpected = null)}>Ignorer</button>
        <button class="primary" onclick={acceptUnexpected}>L'ajouter quand même</button>
      {/snippet}
    </Modal>
  {/if}

  {#if excusing}
    <Modal title="Absence justifiée" onclose={() => (excusing = null)}>
      <p>{excusing.student.last_name.toUpperCase()} {excusing.student.first_name}</p>
      <label class="field">Motif <input bind:value={excuseReason} placeholder="Certificat médical, convocation…" /></label>
      {#snippet footer()}
        <button onclick={() => (excusing = null)}>Annuler</button>
        <button class="primary" onclick={saveExcuse}>Enregistrer</button>
      {/snippet}
    </Modal>
  {/if}
{/if}

<style>
  .pick { display: flex; justify-content: space-between; align-items: center; text-align: left; width: 100%; white-space: normal; }
  .live { display: grid; grid-template-columns: 300px 1fr; gap: 1rem; margin-bottom: 1rem; }
  .counters { display: flex; flex-direction: column; gap: 0.6rem; }
  .big { font-size: 2.6rem; font-weight: 800; margin-right: 0.4rem; font-variant-numeric: tabular-nums; }
  .bar { height: 8px; border-radius: 99px; background: var(--none-bg); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--present); transition: width 0.3s; }
  .banner { display: flex; flex-direction: column; justify-content: center; min-height: 120px; border-left: 8px solid var(--border); transition: background 0.2s; }
  .who { font-size: 1.8rem; font-weight: 700; }
  .present { background: var(--present-bg); border-left-color: var(--present); }
  .late { background: var(--late-bg); border-left-color: var(--late); }
  .absent { background: var(--absent-bg); border-left-color: var(--absent); }
  .excused { background: var(--excused-bg); border-left-color: var(--excused); }
  .mock { margin-bottom: 1rem; padding: 0.5rem 0.8rem; border: 1px dashed var(--border); border-radius: 8px; }
  .cols { display: grid; grid-template-columns: 1fr 280px; gap: 1rem; align-items: start; }
  .tools h2 { margin: 0; }
  .tools { margin-bottom: 0.5rem; }
  aside h2 { margin-bottom: 0.5rem; }
  .feed { padding: 0.45rem 0.7rem; border-radius: 8px; margin-bottom: 0.4rem; border-left: 4px solid var(--border); background: var(--surface); }

</style>
