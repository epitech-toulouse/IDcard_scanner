<script lang="ts">
  import { onMount } from "svelte";
  import { api, events } from "../lib/api";
  import { confirmAction, guard, navigate, reader, setActive, toast } from "../lib/app.svelte";
  import { exportSession } from "../lib/actions";
  import { endOfDay, fmtRange, fmtTime, fmtTimeSec, isToday, startOfDay, toNaive } from "../lib/format";
  import Modal from "../components/Modal.svelte";
  import Icon from "../components/Icon.svelte";
  import StudentPicker from "../components/StudentPicker.svelte";
  import Projection from "../components/Projection.svelte";
  import { describeScan, scanSession, studentName as name } from "../lib/scan";
  import { openInTagOrder, tagFor } from "../lib/sessionTags";
  import {
    SOURCE_LABEL, STATE_LABEL, STATUS_LABEL,
    type AttendanceRow, type Id, type ScanOutcome, type SessionDetail, type SessionRow, type Status, type Student,
  } from "../lib/types";

  let { sessionId }: { sessionId: Id | null } = $props();

  let detail = $state<SessionDetail | null>(null);
  /** Créneaux ouverts (tous reçoivent les badges), dans l'ordre de leurs repères A, B, C… */
  let open = $state<SessionRow[]>([]);
  /** Créneaux d'aujourd'hui pas encore ouverts : « Plus tard aujourd'hui », « Ouvrir un autre créneau ». */
  let laterToday = $state<SessionRow[]>([]);
  let candidates = $state<SessionRow[]>([]);
  type FeedItem = { id: number; cls: string; title: string; sub: string; at: string; sid: Id | null };
  let feed = $state<FeedItem[]>([]);
  let flash = $state<{ cls: string; title: string; sub: string; sid: Id | null; orphan: boolean } | null>(null);
  let unknownUid = $state<string | null>(null);
  let notExpected = $state<Student | null>(null);
  let addTo = $state<Id | null>(null);
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
  /** Cartes affichées : les créneaux ouverts, plus le créneau consulté s'il n'est pas (ou plus) ouvert. */
  let strip = $derived.by(() => {
    if (!s || !open.length) return open;
    return open.some((o) => o.id === s.id) ? open : [...open, s];
  });
  let multi = $derived(strip.length >= 2);
  let tag = (id: Id | null) => tagFor(open, id);
  let sessionById = (id: Id | null) => strip.find((r) => r.id === id) ?? null;
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
  const norm = (t: string) => t.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
  let shownRows = $derived.by(() => {
    const q = norm(search.trim());
    return (detail?.rows ?? []).filter(
      (r) => FILTERS[filter](r) && (!q || norm(`${r.student.last_name} ${r.student.first_name} ${r.groups}`).includes(q)),
    );
  });
  const pctOf = (r: SessionRow, n: number) => (r.counts.expected ? Math.min(100, (100 * n) / r.counts.expected) : 0);
  const hereOf = (r: SessionRow) => r.counts.present + r.counts.late;
  const missingOf = (r: SessionRow) => Math.max(0, r.counts.expected - r.counts.present - r.counts.late - r.counts.absent - r.counts.excused);
  let exportMenu = $state<HTMLDetailsElement>();
  let openMenu = $state<HTMLDetailsElement>();

  async function loadDetail() {
    if (sessionId === null) return;
    detail = (await guard(api.sessionDetail(sessionId))) ?? null;
  }

  /** Créneaux ouverts et créneaux d'aujourd'hui pas encore ouverts. */
  async function loadSessions() {
    const now = new Date();
    const [all, today] = await Promise.all([guard(api.listSessions()), guard(api.listSessions(startOfDay(now), endOfDay(now)))]);
    open = openInTagOrder(all ?? []);
    laterToday = (today ?? []).filter((r) => r.state === "planned").reverse();
    return all ?? [];
  }

  async function load() {
    await Promise.all([loadDetail(), loadSessions()]);
  }

  onMount(() => {
    (async () => {
      if (sessionId === null) {
        const all = await loadSessions();
        const active = await api.getActiveSession();
        const first = active ?? open[0]?.id ?? null;
        if (first !== null) return navigate(`emargement/${first}`);
        const today = new Date();
        candidates = all.filter((r) => r.state === "open" || (isToday(r.starts_at, today) && r.state !== "closed")).reverse();
        return;
      }
      await load();
      // Un créneau ouvert affiché ici devient le créneau préféré (étudiant attendu dans plusieurs).
      if (detail?.session.state === "open") await setActive(sessionId);
    })();
    const off = events.onCard(onCard);
    const offAuto = events.onSessionAuto(() => load());
    return () => {
      off.then((f) => f());
      offAuto.then((f) => f());
      clearTimeout(flashTimer);
    };
  });

  function push(cls: string, title: string, sub: string, sid: Id | null, orphan = false) {
    flash = { cls, title, sub, sid, orphan };
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = null), orphan ? 8000 : 4000);
    feed = [{ id: feedId++, cls, title, sub, at: fmtTimeSec(toNaive(new Date())), sid }, ...feed].slice(0, 30);
  }

  async function onCard(o: ScanOutcome) {
    if (sessionId === null) return;
    const m = describeScan(o);
    push(m.cls, m.title, m.sub, scanSession(o), o.kind === "not_expected");
    if (o.kind === "not_expected") {
      notExpected = o.student;
      addTo = open.some((r) => r.id === sessionId) ? sessionId : (open[0]?.id ?? null);
    }
    if (o.kind === "unknown_card") unknownUid = o.uid;
    await load();
  }

  function pick(id: Id) {
    if (id === sessionId) return;
    navigate(`emargement/${id}`);
  }

  async function openIt(id: Id | null = sessionId) {
    if (id === null) return;
    const row = id === sessionId ? s : laterToday.find((r) => r.id === id);
    if ((await guard(row?.state === "closed" ? api.reopenSession(id) : api.openSession(id))) === undefined) return;
    toast(`Émargement ouvert${row ? ` : ${row.title}` : ""}`, "success");
    if (openMenu) openMenu.open = false;
    if (id === sessionId) await load();
    else navigate(`emargement/${id}`);
  }

  async function closeIt() {
    if (sessionId === null || !s) return;
    const msg = missing
      ? `${missing} étudiant(s) attendu(s) n'ont pas badgé et seront marqués absents.`
      : "Tous les attendus ont un statut.";
    if (!(await confirmAction(`Clore « ${s.title} » ?\n\n${msg}\nVous pourrez encore corriger ou rouvrir.`))) return;
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
    if (addTo === null || !notExpected) return;
    const st = notExpected;
    const target = addTo;
    const status = await guard(api.acceptUnexpected(target, st.id));
    if (status) {
      notExpected = null;
      push(status, name(st), `Ajouté · ${STATUS_LABEL[status].toLowerCase()}`, target);
      await load();
    }
  }

  async function inject() {
    if ((await guard(api.injectCard(mockUid))) !== undefined) mockUid = "";
  }
</script>

{#snippet sessionActions(compact: boolean)}
  {#if s}
    {#if s.state === "open" && !compact}
      <button onclick={() => (projection = true)} title="Plein écran lisible de loin (Échap pour quitter)"><Icon name="monitor" size={16} />Projeter</button>
    {/if}
    <details class="menu" bind:this={exportMenu}>
      <summary class="btn" class:compact><Icon name="download" size={16} />Exporter<Icon name="chevronDown" size={14} stroke={2} /></summary>
      <div class="menu-pop">
        <button onclick={() => { exportMenu!.open = false; exportSession(s, "xlsx"); }}>Classeur Excel (.xlsx)</button>
        <button onclick={() => { exportMenu!.open = false; exportSession(s, "csv"); }}>CSV (séparateur ;)</button>
      </div>
    </details>
    {#if s.state === "open"}
      <button class="strong" class:compact onclick={closeIt}>{compact ? "Clore ce créneau" : "Clore l'émargement"}</button>
    {:else}
      <button class="primary" class:compact onclick={() => openIt()}>{s.state === "closed" ? "Rouvrir" : "Ouvrir l'émargement"}</button>
    {/if}
  {/if}
{/snippet}

{#snippet openAnother()}
  {#if open.length}
    <details class="menu" bind:this={openMenu}>
      <summary class="btn"><Icon name="plus" size={16} stroke={2} />Ouvrir un autre créneau</summary>
      <div class="menu-pop">
        {#each laterToday.filter((l) => l.id !== sessionId) as l (l.id)}
          <button onclick={() => openIt(l.id)}>
            <span class="menu-item"><strong>{l.title}</strong><span class="small muted">{fmtRange(l.starts_at, l.ends_at)}{l.room ? ` · ${l.room}` : ""}</span></span>
          </button>
        {:else}
          <p class="small muted menu-empty">Aucun autre créneau prévu aujourd'hui.</p>
        {/each}
        <button onclick={() => navigate("creneaux")}>Tous les créneaux…</button>
      </div>
    </details>
  {/if}
{/snippet}

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
      {#if multi}
        <h1 class="title-line">{open.length} émargement{open.length > 1 ? "s" : ""} en cours</h1>
        <p class="muted">Chaque badge va automatiquement dans le créneau où l'étudiant est attendu.</p>
      {:else}
        <div class="row title-line">
          <h1>{s.title}</h1>
          <span class="pill {s.state}">{STATE_LABEL[s.state]}</span>
        </div>
        <p class="muted">{fmtRange(s.starts_at, s.ends_at)}{s.room ? ` · ${s.room}` : ""} · {s.targets} · retard après {s.late_after_min} min</p>
      {/if}
    </div>
    <div class="row">
      {#if multi}
        {#if s.state === "open"}
          <button onclick={() => (projection = true)} title="Plein écran lisible de loin (Échap pour quitter)"><Icon name="monitor" size={16} />Projeter</button>
        {/if}
        {@render openAnother()}
      {:else}
        {#if s.state === "open"}{@render openAnother()}{/if}
        {@render sessionActions(false)}
      {/if}
    </div>
  </div>

  {#if multi}
    <section class="strip" aria-label="Créneaux ouverts">
      {#each strip as r (r.id)}
        {@const t = tag(r.id)}
        <button class="scard {t.cls}" class:on={r.id === sessionId} aria-pressed={r.id === sessionId} onclick={() => pick(r.id)}>
          <span class="scard-head">
            <span class="tag lg">{t.letter}</span>
            <span class="scard-title">
              <strong>{r.title}</strong>
              <span class="small muted">{fmtTime(r.starts_at)} – {fmtTime(r.ends_at)}{r.room ? ` · ${r.room}` : ""} · {r.targets}</span>
            </span>
            <span class="pill {r.state}">{STATE_LABEL[r.state]}</span>
          </span>
          <span class="count-line">
            <span class="mid">{hereOf(r)}</span><span class="muted">/ {r.counts.expected} présents</span>
            <span class="grow"></span>
            <span class="small muted">{missingOf(r)} non badgé{missingOf(r) > 1 ? "s" : ""}</span>
          </span>
          <span class="bar">
            <span class="ok" style:width="{pctOf(r, r.counts.present)}%"></span>
            <span class="lt" style:width="{pctOf(r, r.counts.late)}%"></span>
          </span>
        </button>
      {/each}
      {#if laterToday.length}
        <div class="later">
          <span class="kicker muted">Plus tard aujourd'hui</span>
          {#each laterToday.slice(0, 2) as l (l.id)}
            <button class="later-item" onclick={() => navigate(`emargement/${l.id}`)}>
              <strong>{l.title}</strong><span class="small muted">{fmtRange(l.starts_at, l.ends_at)}{l.room ? ` · ${l.room}` : ""}</span>
            </button>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  <div class="live">
    {#if !multi}
      <section class="card counters" aria-label="Avancement">
        <div class="count-line"><span class="big">{here}</span><span class="muted">/ {s.counts.expected} présents</span></div>
        <div class="bar" role="img" aria-label="{here} sur {s.counts.expected} présents, dont {s.counts.late} en retard">
          <span class="ok" style:width="{pctOf(s, s.counts.present)}%"></span>
          <span class="lt" style:width="{pctOf(s, s.counts.late)}%"></span>
        </div>
        <div class="row chips">
          <span class="pill present">{s.counts.present} à l'heure</span>
          <span class="pill late">{s.counts.late} en retard</span>
          <span class="pill">{missing} non badgé{missing > 1 ? "s" : ""}</span>
          {#if s.counts.excused}<span class="pill excused">{s.counts.excused} justifié{s.counts.excused > 1 ? "s" : ""}</span>{/if}
          {#if s.counts.absent}<span class="pill absent">{s.counts.absent} absent{s.counts.absent > 1 ? "s" : ""}</span>{/if}
        </div>
      </section>
    {/if}

    <section class="banner {flash?.cls ?? 'idle'}" aria-live="polite" aria-label="Dernier passage">
      <span class="avatar"><Icon name="nfc" size={28} /></span>
      <div class="grow">
        {#if flash}
          {@const fs = sessionById(flash.sid)}
          <div class="kicker">{flash.orphan ? "Non attendu" : "Dernier passage"}</div>
          <div class="who">{flash.title}</div>
          <div class="row sub-line">
            <span>{flash.sub}</span>
            {#if fs && (multi || fs.id !== sessionId)}
              <span class="went {tag(fs.id).cls}"><Icon name="chevronRight" size={14} stroke={2.6} />{tag(fs.id).letter} · {fs.title}{fs.room ? ` · ${fs.room}` : ""}</span>
            {/if}
          </div>
        {:else if open.length}
          <div class="kicker">En attente</div>
          <div class="who">Badgez votre carte étudiant</div>
          <div class="muted">
            {reader.connected ? reader.message : "Lecteur non connecté : voir Réglages"}{multi ? ` · les ${open.length} créneaux ouverts reçoivent les badges` : ""}
          </div>
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
      {#if multi}
        <div class="list-head">
          <span class="tag {tag(s.id).cls}">{tag(s.id).letter}</span>
          <strong>{s.title}</strong>
          <span class="small muted">{s.targets} · retard après {s.late_after_min} min</span>
          <span class="grow"></span>
          {@render sessionActions(true)}
        </div>
      {/if}
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
      <div class="card-head"><h2 id="h-feed">Derniers passages</h2>{#if multi}<span class="small muted">tous créneaux</span>{/if}</div>
      <div class="feed-list">
        {#each feed as f (f.id)}
          {@const fs = sessionById(f.sid)}
          <div class="feed {f.cls}">
            <span class="dot"></span>
            <span class="grow"><strong>{f.title}</strong><span class="small">{f.sub}</span></span>
            <span class="feed-meta">
              {#if multi}
                <span class="tag-chip {tag(f.sid).cls}">{fs ? `${tag(f.sid).letter} · ${fs.title}` : "Aucun créneau"}</span>
              {/if}
              <span class="small muted">{f.at}</span>
            </span>
          </div>
        {:else}
          <p class="muted small">Les badges apparaîtront ici.</p>
        {/each}
      </div>
    </aside>
  </div>

  {#if projection}
    <Projection
      session={s}
      {here}
      {flash}
      {recent}
      flashTag={flash && multi && sessionById(flash.sid) ? { ...tag(flash.sid), title: sessionById(flash.sid)!.title, room: sessionById(flash.sid)!.room } : null}
      others={multi ? open.map((r) => ({ ...tag(r.id), id: r.id, title: r.title, room: r.room, here: hereOf(r), expected: r.counts.expected })) : []}
      onexit={() => (projection = false)}
    />
  {/if}

  {#if unknownUid}
    <Modal title="Carte inconnue" onclose={() => (unknownUid = null)}>
      <p>La carte <span class="mono">{unknownUid}</span> n'est associée à personne. À qui appartient-elle ?</p>
      <StudentPicker onpick={linkUnknown} preferIds={expectedRows.filter((r) => !r.status).map((r) => r.student.id)} />
    </Modal>
  {/if}

  {#if notExpected}
    <Modal title="Étudiant non attendu" onclose={() => (notExpected = null)}>
      {#if open.length}
        <p><strong>{name(notExpected)}</strong> n'est attendu dans aucun des créneaux ouverts. L'ajouter à :</p>
        <fieldset class="choices">
          <legend class="sr-only">Créneau</legend>
          {#each open as r (r.id)}
            <label class="choice" class:on={addTo === r.id}>
              <input type="radio" name="add-to" value={r.id} bind:group={addTo} />
              <span class="tag {tag(r.id).cls}">{tag(r.id).letter}</span>
              <span class="grow"><strong>{r.title}</strong><br /><span class="small muted">{fmtRange(r.starts_at, r.ends_at)}{r.room ? ` · ${r.room}` : ""} · {r.targets}</span></span>
            </label>
          {/each}
        </fieldset>
        <p class="small muted">Le statut suit l'heure et la règle de retard du créneau choisi.</p>
      {:else}
        <p><strong>{name(notExpected)}</strong> : aucun créneau n'est ouvert.</p>
      {/if}
      {#snippet footer()}
        <button onclick={() => (notExpected = null)}>Ignorer</button>
        {#if open.length}
          <button class="primary" disabled={addTo === null} onclick={acceptUnexpected}>
            Ajouter{addTo !== null && sessionById(addTo) ? ` à ${tag(addTo).letter} · ${sessionById(addTo)!.title}` : ""}
          </button>
        {/if}
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
    position: absolute; right: 0; top: calc(100% + 4px); z-index: 20; min-width: 16rem;
    background: var(--surface); border: 1px solid var(--border); border-radius: 10px; padding: 4px;
    box-shadow: 0 8px 24px rgb(16 24 40 / 14%); display: flex; flex-direction: column;
  }
  .menu-pop button { border: 0; justify-content: flex-start; background: transparent; text-align: left; white-space: normal; height: auto; min-height: 2.4rem; padding: 0.4rem 0.75rem; }
  .menu-item { display: flex; flex-direction: column; }
  .menu-empty { margin: 0; padding: 0.5rem 0.75rem; }
  .compact { min-height: 2.2rem; font-size: 0.92rem; }

  .strip { display: grid; grid-template-columns: repeat(auto-fit, minmax(290px, 1fr)); gap: 0.9rem; margin-bottom: 1.2rem; }
  .scard {
    display: flex; flex-direction: column; gap: 0.7rem; align-items: stretch; text-align: left; white-space: normal; height: auto;
    padding: 0.9rem 1rem; border-radius: var(--radius); background: var(--surface); border: 1px solid var(--border); margin: 1px;
  }
  .scard:hover:not(:disabled) { background: var(--surface); border-color: var(--border-strong); }
  .scard.on, .scard.on:hover:not(:disabled) { border: 2px solid var(--tag); margin: 0; box-shadow: 0 0 0 4px var(--tag-soft); }
  .scard-head { display: flex; align-items: center; gap: 0.7rem; }
  .scard-title { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .scard-title strong { font-size: 1.05rem; }
  .mid { font-size: 2rem; font-weight: 800; letter-spacing: -0.02em; line-height: 1; }
  .later { border: 1px dashed var(--border-strong); border-radius: var(--radius); padding: 0.8rem 1rem; display: flex; flex-direction: column; gap: 0.35rem; justify-content: center; }
  .later-item { border: 0; background: transparent; padding: 0.25rem 0; height: auto; min-height: 0; justify-content: flex-start; text-align: left; display: flex; flex-wrap: wrap; gap: 0.2rem 0.6rem; white-space: normal; }
  .later-item:hover:not(:disabled) { background: transparent; text-decoration: underline; }

  .live { display: flex; flex-wrap: wrap; gap: 1.2rem; margin-bottom: 1.2rem; }
  .counters { flex: 1 1 300px; display: flex; flex-direction: column; gap: 0.75rem; }
  .count-line { display: flex; align-items: baseline; gap: 0.5rem; }
  .big { font-size: 2.9rem; font-weight: 800; letter-spacing: -0.02em; line-height: 1; }
  .bar { height: 10px; border-radius: 99px; background: var(--none-bg); overflow: hidden; display: flex; }
  .scard .bar { height: 8px; }
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
  .sub-line { gap: 0.6rem; }
  .went { display: inline-flex; align-items: center; gap: 0.35rem; padding: 0.2rem 0.75rem; border-radius: 99px; background: var(--tag); color: #fff; font-weight: 700; font-size: 0.9rem; }
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
  .list-head { display: flex; flex-wrap: wrap; align-items: center; gap: 0.6rem; padding: 0.8rem 1.1rem; border-bottom: 1px solid var(--border); }
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
  .feed-meta { display: flex; flex-direction: column; align-items: flex-end; gap: 0.25rem; }
  .feed .dot { width: 0.55rem; height: 0.55rem; border-radius: 50%; background: var(--border-strong); margin-top: 0.45rem; flex: none; }
  .feed.present .dot { background: var(--present-bar); }
  .feed.late .dot { background: var(--late-bar); }
  .feed.absent .dot { background: var(--absent); }
  .feed.excused .dot { background: var(--excused); }

  .choices { border: 0; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.5rem; }
  .choice { display: flex; align-items: center; gap: 0.75rem; padding: 0.7rem 0.9rem; border-radius: 12px; border: 1px solid var(--border); margin: 1px; cursor: pointer; }
  .choice.on { border: 2px solid var(--accent); margin: 0; background: var(--accent-soft); }
  .choice input { width: 1.1rem; height: 1.1rem; accent-color: var(--accent); margin: 0; }
</style>
