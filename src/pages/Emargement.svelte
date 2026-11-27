<script lang="ts">
  import { onMount } from "svelte";
  import { api, events } from "../lib/api";
  import { confirmAction, guard, navigate, reader, setActive, toast } from "../lib/app.svelte";
  import { exportSession } from "../lib/actions";
  import { fmtRange, fmtTime, fmtTimeSec, isToday, toNaive } from "../lib/format";
  import Modal from "../components/Modal.svelte";
  import Icon from "../components/Icon.svelte";
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
  type Filter = "all" | "missing" | "late" | "off";
  let filter = $state<Filter>("all");
  let search = $state("");
  let projection = $state(false);
  let mockUid = $state("");
  let feedId = 0;
  let flashTimer: ReturnType<typeof setTimeout>;

  let s = $derived(detail?.session ?? null);
  let expectedRows = $derived(detail?.rows.filter((r) => r.expected) ?? []);
  let here = $derived(detail?.rows.filter((r) => r.status === "present" || r.status === "late").length ?? 0);
  let missing = $derived(expectedRows.filter((r) => !r.status).length);
  let recent = $derived(feed.filter((f, i) => (f.cls === "present" || f.cls === "late") && feed.findIndex((g) => g.title === f.title) === i).slice(0, 8));
  const FILTERS: Record<Filter, (r: AttendanceRow) => boolean> = {
    all: () => true,
    missing: (r) => !r.status,
    late: (r) => r.status === "late",
    off: (r) => r.status === "absent" || r.status === "excused",
  };
  const TABS: { id: Filter; label: string }[] = [
    { id: "all", label: "Tous" },
    { id: "missing", label: "Non badgés" },
    { id: "late", label: "Retards" },
    { id: "off", label: "Absents · justifiés" },
  ];
  const SEGMENTS: { status: Status; label: string }[] = [
    { status: "present", label: "Présent" },
    { status: "late", label: "Retard" },
    { status: "absent", label: "Absent" },
    { status: "excused", label: "Justifié" },
  ];
  const norm = (t: string) => t.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
  let shownRows = $derived.by(() => {
    const q = norm(search.trim());
    return (detail?.rows ?? []).filter(
      (r) => FILTERS[filter](r) && (!q || norm(`${r.student.last_name} ${r.student.first_name} ${r.groups}`).includes(q)),
    );
  });
  let pctOf = (n: number) => (s && s.counts.expected ? Math.min(100, (100 * n) / s.counts.expected) : 0);
  let exportMenu = $state<HTMLDetailsElement>();

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
      if (detail?.session.state === "open") await setActive(sessionId);
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
      filter = "all";
      projection = false;
      await load();
    }
  }

  async function setStatus(r: AttendanceRow, value: string) {
    if (sessionId === null) return;
    if (value === r.status && value !== "excused") value = "clear";
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
    <div class="head-info">
      <div class="small muted"><a href="#/accueil">Aujourd'hui</a> / Émargement</div>
      <div class="row title-line">
        <h1>{s.title}</h1>
        <span class="pill {s.state}">{STATE_LABEL[s.state]}</span>
      </div>
      <p class="muted">{fmtRange(s.starts_at, s.ends_at)}{s.room ? ` · ${s.room}` : ""} · {s.targets} · retard après {s.late_after_min} min</p>
    </div>
    <div class="row">
      {#if s.state === "open"}
        <button onclick={() => (projection = true)} title="Plein écran lisible de loin (Échap pour quitter)"><Icon name="monitor" size={16} />Projeter</button>
      {/if}
      <details class="menu" bind:this={exportMenu}>
        <summary class="btn"><Icon name="download" size={16} />Exporter<Icon name="chevronDown" size={14} stroke={2} /></summary>
        <div class="menu-pop">
          <button onclick={() => { exportMenu!.open = false; exportSession(s, "xlsx"); }}>Classeur Excel (.xlsx)</button>
          <button onclick={() => { exportMenu!.open = false; exportSession(s, "csv"); }}>CSV (séparateur ;)</button>
        </div>
      </details>
      {#if s.state === "open"}
        <button class="strong" onclick={closeIt}>Clore l'émargement</button>
      {:else}
        <button class="primary" onclick={openIt}>{s.state === "closed" ? "Rouvrir" : "Ouvrir l'émargement"}</button>
      {/if}
    </div>
  </div>

  <div class="live">
    <section class="card counters" aria-label="Avancement">
      <div class="count-line"><span class="big">{here}</span><span class="muted">/ {s.counts.expected} présents</span></div>
      <div class="bar" role="img" aria-label="{here} sur {s.counts.expected} présents, dont {s.counts.late} en retard">
        <span class="ok" style:width="{pctOf(s.counts.present)}%"></span>
        <span class="lt" style:width="{pctOf(s.counts.late)}%"></span>
      </div>
      <div class="row chips">
        <span class="pill present">{s.counts.present} à l'heure</span>
        <span class="pill late">{s.counts.late} en retard</span>
        <span class="pill">{missing} non badgé{missing > 1 ? "s" : ""}</span>
        {#if s.counts.excused}<span class="pill excused">{s.counts.excused} justifié{s.counts.excused > 1 ? "s" : ""}</span>{/if}
        {#if s.counts.absent}<span class="pill absent">{s.counts.absent} absent{s.counts.absent > 1 ? "s" : ""}</span>{/if}
      </div>
    </section>

    <section class="banner {flash?.cls ?? 'idle'}" aria-live="polite" aria-label="Dernier passage">
      <span class="avatar"><Icon name="nfc" size={28} /></span>
      <div class="grow">
        {#if flash}
          <div class="kicker">Dernier passage</div>
          <div class="who">{flash.title}</div>
          <div>{flash.sub}</div>
        {:else if s.state === "open"}
          <div class="kicker">En attente</div>
          <div class="who">Badgez votre carte étudiant</div>
          <div class="muted">{reader.connected ? reader.message : "Lecteur non connecté : voir Réglages"}</div>
        {:else}
          <div class="who muted">Émargement {s.state === "closed" ? "clos" : "non ouvert"}</div>
          {#if s.state === "planned"}<div class="muted">Ouvrez-le pour enregistrer les badges.</div>{/if}
        {/if}
      </div>
      {#if reader.backend === "mock"}
        <form class="mock" onsubmit={(e) => { e.preventDefault(); inject(); }}>
          <label class="sr-only" for="mock-uid">UID à badger</label>
          <input id="mock-uid" bind:value={mockUid} placeholder="UID (ex. 04:24:A5:82:E6:19:90)" size="26" />
          <button>Badger</button>
        </form>
      {/if}
    </section>
  </div>

  <div class="cols">
    <section class="card flush" aria-label="Liste des étudiants">
      <div class="list-tools">
        <div class="tabs" role="tablist" aria-label="Filtrer la liste">
          {#each TABS as t (t.id)}
            <button role="tab" aria-selected={filter === t.id} class:on={filter === t.id} onclick={() => (filter = t.id)}>
              {t.label} <span class="count">{detail?.rows.filter(FILTERS[t.id]).length ?? 0}</span>
            </button>
          {/each}
        </div>
        <span class="grow"></span>
        <label class="search">
          <Icon name="search" size={16} stroke={2} />
          <span class="sr-only">Chercher dans la liste</span>
          <input type="search" bind:value={search} placeholder="Chercher dans la liste" />
        </label>
      </div>
      <div class="table-scroll">
        <table>
          <thead><tr><th>Étudiant</th><th>Groupe(s)</th><th>Passage</th><th>Statut</th></tr></thead>
          <tbody>
            {#each shownRows as r (r.student.id)}
              <tr>
                <td>
                  <div class="who-cell">
                    <span class="initials" aria-hidden="true">{r.student.first_name.charAt(0)}{r.student.last_name.charAt(0)}</span>
                    <span>
                      <strong>{r.student.last_name.toUpperCase()}</strong> {r.student.first_name}
                      {#if !r.expected}<span class="pill planned">non attendu</span>{/if}
                      {#if r.reason}<span class="small muted reason">{r.reason}</span>{/if}
                    </span>
                  </div>
                </td>
                <td class="small">{r.groups}</td>
                <td class="small" class:muted={!r.status}>
                  {#if r.status}{fmtTimeSec(r.scanned_at)}{r.scanned_at && r.source ? " · " : ""}{r.source ? SOURCE_LABEL[r.source] : ""}{:else}Non badgé{/if}
                </td>
                <td>
                  <div class="seg" role="radiogroup" aria-label="Statut de {r.student.first_name} {r.student.last_name}">
                    {#each SEGMENTS as g (g.status)}
                      <button
                        role="radio"
                        aria-checked={r.status === g.status}
                        class="{g.status}"
                        class:on={r.status === g.status}
                        title={r.status === g.status && g.status !== "excused" ? "Cliquer à nouveau pour effacer" : STATUS_LABEL[g.status]}
                        onclick={() => setStatus(r, g.status)}
                      >{g.label}</button>
                    {/each}
                  </div>
                </td>
              </tr>
            {:else}
              <tr><td colspan="4" class="empty">
                {#if filter === "missing"}Tout le monde a un statut.{:else if detail?.rows.length}Personne dans cette catégorie.{:else}Aucun étudiant attendu : vérifiez les groupes du créneau.{/if}
              </td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
    <aside class="card flush feed-card" aria-labelledby="h-feed">
      <div class="card-head"><h2 id="h-feed">Derniers passages</h2></div>
      <div class="feed-list">
        {#each feed as f (f.id)}
          <div class="feed {f.cls}"><span class="dot"></span><span class="grow"><strong>{f.title}</strong><span class="small">{f.sub}</span></span><span class="small muted">{f.at}</span></div>
        {:else}
          <p class="muted small">Les badges apparaîtront ici.</p>
        {/each}
      </div>
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
  .pick { display: flex; justify-content: space-between; align-items: center; text-align: left; width: 100%; white-space: normal; min-height: 4rem; padding: 0.9rem 1.2rem; }
  .head-info { flex: 1 1 360px; min-width: 0; }
  .head-info a { text-decoration: none; }
  .title-line { margin: 0.15rem 0; }
  .title-line h1 { margin: 0; }

  .menu { position: relative; }
  .menu summary { list-style: none; }
  .menu summary::-webkit-details-marker { display: none; }
  .menu-pop {
    position: absolute; right: 0; top: calc(100% + 4px); z-index: 20; min-width: 14rem;
    background: var(--surface); border: 1px solid var(--border); border-radius: 10px; padding: 4px;
    box-shadow: 0 8px 24px rgb(16 24 40 / 14%); display: flex; flex-direction: column;
  }
  .menu-pop button { border: 0; justify-content: flex-start; background: transparent; }

  .live { display: flex; flex-wrap: wrap; gap: 1.2rem; margin-bottom: 1.2rem; }
  .counters { flex: 1 1 300px; display: flex; flex-direction: column; gap: 0.75rem; }
  .count-line { display: flex; align-items: baseline; gap: 0.5rem; }
  .big { font-size: 2.9rem; font-weight: 800; letter-spacing: -0.02em; line-height: 1; }
  .bar { height: 10px; border-radius: 99px; background: var(--none-bg); overflow: hidden; display: flex; }
  .bar span { transition: width 0.3s; }
  .ok { background: var(--present-bar); }
  .lt { background: var(--late-bar); }
  .chips { gap: 0.4rem; }

  .banner {
    flex: 999 1 420px; display: flex; align-items: center; gap: 1.2rem; flex-wrap: wrap;
    border-radius: var(--radius); padding: 1.3rem 1.6rem; min-height: 8rem;
    background: var(--surface); border: 1px solid var(--border); transition: background 0.2s;
  }
  .banner .kicker { color: var(--accent); }
  .avatar { width: 4rem; height: 4rem; border-radius: 50%; background: var(--surface); color: var(--accent); display: grid; place-items: center; flex: none; border: 1px solid var(--border); }
  .who { font-size: 1.75rem; font-weight: 700; line-height: 1.2; margin: 0.1rem 0; }
  .banner.present { background: var(--present-bg); border-color: var(--present-bd); }
  .banner.late { background: var(--late-bg); border-color: var(--late-bd); }
  .banner.absent { background: var(--absent-bg); border-color: var(--absent-bd); }
  .banner.excused { background: var(--excused-bg); border-color: var(--excused-bd); }
  .banner.present :is(.kicker, .avatar) { color: var(--present); }
  .banner.late :is(.kicker, .avatar) { color: var(--late); }
  .banner.absent :is(.kicker, .avatar) { color: var(--absent); }
  .banner.excused :is(.kicker, .avatar) { color: var(--excused); }
  .mock { display: flex; gap: 0.5rem; padding: 0.5rem; border: 1px dashed var(--border-strong); border-radius: 10px; background: var(--surface); }

  .cols { display: flex; flex-wrap: wrap; gap: 1.2rem; align-items: flex-start; }
  .cols > section { flex: 999 1 640px; min-width: 0; }
  .feed-card { flex: 1 1 260px; min-width: 0; }
  .list-tools { display: flex; flex-wrap: wrap; align-items: center; gap: 0.8rem; padding: 0.9rem 1.1rem; border-bottom: 1px solid var(--border); }
  .search { display: flex; align-items: center; gap: 0.5rem; border: 1px solid var(--border-strong); border-radius: 8px; padding: 0 0.75rem; color: var(--muted); flex: 0 1 260px; }
  .search input { border: 0; padding: 0.55rem 0; background: transparent; flex: 1; outline: none; color: var(--text); }
  .search:focus-within { outline: 2px solid var(--accent); outline-offset: 1px; }
  .table-scroll { overflow-x: auto; }
  .table-scroll th { position: static; }
  .who-cell { display: flex; align-items: center; gap: 0.75rem; }
  .initials { width: 2.2rem; height: 2.2rem; border-radius: 50%; background: var(--accent-soft); color: var(--side-active-text); display: grid; place-items: center; font-size: 0.8rem; font-weight: 700; flex: none; text-transform: uppercase; }
  .reason { display: block; }

  .seg { display: flex; gap: 4px; flex-wrap: wrap; }
  .seg button { min-height: 2.2rem; padding: 0 0.75rem; font-size: 0.9rem; font-weight: 600; color: var(--text-2); border-color: var(--border); }
  .seg button.on.present { background: var(--present-bg); color: var(--present); border-color: var(--present-bd); }
  .seg button.on.late { background: var(--late-bg); color: var(--late); border-color: var(--late-bd); }
  .seg button.on.absent { background: var(--absent-bg); color: var(--absent); border-color: var(--absent-bd); }
  .seg button.on.excused { background: var(--excused-bg); color: var(--excused); border-color: var(--excused-bd); }

  .feed-list { padding: 0.8rem; display: flex; flex-direction: column; gap: 0.3rem; max-height: 32rem; overflow-y: auto; }
  .feed-list p { padding: 0.4rem 0.6rem; margin: 0; }
  .feed { display: flex; align-items: flex-start; gap: 0.6rem; padding: 0.5rem 0.6rem; border-radius: 8px; }
  .feed .grow { display: flex; flex-direction: column; min-width: 0; }
  .feed .dot { width: 0.55rem; height: 0.55rem; border-radius: 50%; background: var(--border-strong); margin-top: 0.45rem; flex: none; }
  .feed.present .dot { background: var(--present-bar); }
  .feed.late .dot { background: var(--late-bar); }
  .feed.absent .dot { background: var(--absent); }
  .feed.excused .dot { background: var(--excused); }
</style>
