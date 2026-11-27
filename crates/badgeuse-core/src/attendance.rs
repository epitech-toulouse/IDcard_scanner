//! Enregistrement des présences : badge, correction manuelle, clôture.

use crate::model::*;
use crate::remote::{self, RemoteRecorded, RemoteReply, RemoteScan};
use crate::{Error, Result, db, domain, sessions, students};
use chrono::NaiveDateTime;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

/// Résultat d'un passage de carte, envoyé tel quel au front.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScanOutcome {
    /// Présence enregistrée (present ou late) sur le créneau `session_id`.
    Recorded { uid: String, student: Student, status: Status, scanned_at: NaiveDateTime, session_id: Id },
    /// L'étudiant avait déjà badgé sur ce créneau.
    AlreadyScanned { uid: String, student: Student, status: Status, session_id: Id },
    /// Carte connue mais étudiant attendu dans aucun créneau ouvert (le prof peut l'ajouter à la main).
    NotExpected { uid: String, student: Student },
    /// Carte jamais vue : à associer à un étudiant.
    UnknownCard { uid: String },
    /// Carte reconnue alors qu'aucun créneau n'est ouvert.
    Identified { uid: String, student: Option<Student> },
    /// Le créneau visé n'est pas ouvert (`record_scan` sur un créneau précis).
    SessionNotOpen { uid: String },
    /// UID aléatoire (téléphone, certaines cartes bancaires) : inutilisable pour identifier.
    RandomUid { uid: String },
}

impl ScanOutcome {
    /// Pour le retour LED/bip du lecteur.
    pub fn feedback(&self) -> crate::nfc::Feedback {
        use crate::nfc::Feedback;
        match self {
            ScanOutcome::Recorded { status: Status::Present, .. } => Feedback::Ok,
            ScanOutcome::AlreadyScanned { .. } => Feedback::Silent,
            ScanOutcome::Recorded { .. } => Feedback::Warn,
            ScanOutcome::Identified { student: Some(_), .. } => Feedback::Ok,
            _ => Feedback::Error,
        }
    }
}

fn existing(c: &Connection, session_id: Id, student_id: Id) -> Result<Option<Status>> {
    Ok(c.query_row(
        "SELECT status FROM attendance WHERE session_id = ?1 AND student_id = ?2",
        [session_id, student_id],
        |r| r.get(0),
    )
    .optional()?)
}

fn upsert(
    c: &Connection,
    session_id: Id,
    student_id: Id,
    status: Status,
    scanned_at: Option<NaiveDateTime>,
    source: Source,
    reason: Option<&str>,
) -> Result<()> {
    c.execute(
        "INSERT INTO attendance(session_id, student_id, status, scanned_at, source, reason, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(session_id, student_id) DO UPDATE SET
           status = excluded.status, scanned_at = COALESCE(excluded.scanned_at, attendance.scanned_at),
           source = excluded.source, reason = excluded.reason, updated_at = excluded.updated_at",
        params![session_id, student_id, status, scanned_at, source, reason, db::now()],
    )?;
    Ok(())
}

/// Identifie une carte sans créneau actif.
pub fn identify(c: &Connection, uid: &str) -> Result<ScanOutcome> {
    Ok(ScanOutcome::Identified { uid: uid.into(), student: students::find_by_card(c, uid)? })
}

/// Traite un passage de carte sur le créneau `session_id`.
pub fn record_scan(c: &Connection, session_id: Id, uid: &str, now: NaiveDateTime) -> Result<ScanOutcome> {
    record_scan_inner(c, session_id, uid, now, false)
}

/// `allow_closed` : badge rejoué par une borne après la clôture du créneau. Il remplace alors
/// l'absence posée à la clôture (`domain::scan_overrides`), exactement comme à la réouverture.
fn record_scan_inner(c: &Connection, session_id: Id, uid: &str, now: NaiveDateTime, allow_closed: bool) -> Result<ScanOutcome> {
    let uid = uid.to_string();
    let session = sessions::get(c, session_id)?;
    let accepts = match session.state {
        SessionState::Open => true,
        SessionState::Closed => allow_closed,
        SessionState::Planned => false,
    };
    if !accepts {
        return Ok(ScanOutcome::SessionNotOpen { uid });
    }
    let Some(student) = students::find_by_card(c, &uid)? else {
        return Ok(ScanOutcome::UnknownCard { uid });
    };
    let prev = existing(c, session_id, student.id)?;
    if !domain::scan_overrides(prev) {
        return Ok(ScanOutcome::AlreadyScanned { uid, student, status: prev.unwrap(), session_id });
    }
    if prev.is_none() && !sessions::expected_ids(c, session_id)?.contains(&student.id) {
        return Ok(ScanOutcome::NotExpected { uid, student });
    }
    let status = domain::status_for_scan(session.starts_at, session.late_after_min, now);
    upsert(c, session_id, student.id, status, Some(now), Source::Badge, None)?;
    Ok(ScanOutcome::Recorded { uid, student, status, scanned_at: now, session_id })
}

/// Badge pris à l'instant `at` : choisit le créneau parmi tous ceux qui sont ouverts
/// (`domain::pick_scan_session`), puis l'enregistre. `focus` : créneau affiché à l'écran, préféré
/// à égalité. `replay` : badge de borne rejoué, qui peut aussi aller sur un créneau clos depuis,
/// si sa fenêtre contient l'heure du badge (l'absence posée à la clôture est alors remplacée).
/// Renvoie aussi le créneau retenu.
fn route_scan(c: &Connection, focus: Option<Id>, uid: &str, at: NaiveDateTime, replay: bool) -> Result<(ScanOutcome, Option<Id>)> {
    let lead = crate::settings::load(c)?.auto_open_lead_min;
    let ids: Vec<Id> = c
        .prepare(
            "SELECT id FROM sessions WHERE state = 'open'
               OR (?1 AND state = 'closed' AND starts_at <= ?2 AND ends_at >= ?3)",
        )?
        .query_map(params![replay, at + chrono::Duration::days(1), at - chrono::Duration::days(1)], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut sessions_found = vec![];
    for id in ids {
        let s = sessions::get(c, id)?;
        if s.state == SessionState::Open || domain::session_window_contains(s.starts_at, s.ends_at, lead, at) {
            sessions_found.push(s);
        }
    }
    let any_open = sessions_found.iter().any(|s| s.state == SessionState::Open);
    let Some(student) = students::find_by_card(c, uid)? else {
        let outcome = if any_open { ScanOutcome::UnknownCard { uid: uid.into() } } else { identify(c, uid)? };
        return Ok((outcome, None));
    };
    let mut candidates = vec![];
    for s in &sessions_found {
        let prev = existing(c, s.id, student.id)?;
        if prev.is_some() || sessions::expected_ids(c, s.id)?.contains(&student.id) {
            candidates.push(domain::ScanCandidate {
                id: s.id,
                starts_at: s.starts_at,
                ends_at: s.ends_at,
                settled: !domain::scan_overrides(prev),
            });
        }
    }
    match domain::pick_scan_session(focus, &candidates, at, lead) {
        Some(sid) => Ok((record_scan_inner(c, sid, uid, at, replay)?, Some(sid))),
        None if any_open => Ok((ScanOutcome::NotExpected { uid: uid.into(), student }, None)),
        None => Ok((ScanOutcome::Identified { uid: uid.into(), student: Some(student) }, None)),
    }
}

/// Badge d'un lecteur du PC : il va au créneau ouvert où l'étudiant est attendu (voir `route_scan`) ;
/// sans créneau ouvert, simple identification.
pub fn scan(c: &Connection, focus: Option<Id>, uid: &str, now: NaiveDateTime) -> Result<ScanOutcome> {
    Ok(route_scan(c, focus, uid, now, false)?.0)
}

/// Traite un badge envoyé par une borne réseau (voir `remote`).
/// - Rejeu (même `scan_id`) : renvoie la réponse d'origine, sans rien réécrire.
/// - Heure ramenée sur l'horloge de l'appli (`remote::correct_clock`).
/// - Créneau : comme un badge du PC (`route_scan`), avec en plus les créneaux clos depuis dont la
///   fenêtre contient l'heure du badge.
pub fn record_remote_scan(c: &Connection, focus: Option<Id>, scan: &RemoteScan, now: NaiveDateTime) -> Result<RemoteRecorded> {
    if scan.scan_id.trim().is_empty() || scan.device.trim().is_empty() {
        return Err(Error::Invalid("Badge distant sans identifiant de borne".into()));
    }
    if let Some(json) = c
        .query_row("SELECT reply_json FROM remote_scans WHERE scan_id = ?1", [&scan.scan_id], |r| r.get::<_, String>(0))
        .optional()?
    {
        let reply = serde_json::from_str(&json).map_err(|e| Error::Invalid(format!("Réponse enregistrée illisible : {e}")))?;
        return Ok(RemoteRecorded { reply, outcome: None });
    }
    let bytes = crate::nfc::parse_uid(&scan.uid).ok_or_else(|| Error::Invalid(format!("UID invalide : {}", scan.uid)))?;
    let card = crate::nfc::CardInfo::new(bytes);
    let uid = card.uid_hex();
    let at = remote::correct_clock(scan.scanned_at, scan.device_now, now);

    let (outcome, session_id) = if card.is_random_uid() {
        (ScanOutcome::RandomUid { uid: uid.clone() }, None)
    } else {
        route_scan(c, focus, &uid, at, true)?
    };

    let mut reply = RemoteReply::from_outcome(&outcome);
    if let Some(sid) = session_id {
        let row = sessions::row(c, sid)?;
        let tag = sessions::open_rows(c)?.iter().position(|r| r.session.id == sid);
        reply.session_title = Some(row.session.title.clone());
        reply.progress = Some(remote::Progress::from_counts(&row.counts));
        reply.session = Some(remote::RemoteSession::from_row(&row, tag));
    }
    c.execute("DELETE FROM remote_scans WHERE received_at < ?1", [remote::purge_before(now)])?;
    c.execute(
        "INSERT INTO remote_scans(scan_id, device, uid, scanned_at, received_at, reply_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![scan.scan_id, scan.device, uid, at, now, serde_json::to_string(&reply).expect("réponse sérialisable")],
    )?;
    Ok(RemoteRecorded { reply, outcome: Some(outcome) })
}

/// Le prof accepte un étudiant non attendu qui vient de badger : statut selon l'heure, source badge.
pub fn accept_unexpected(c: &Connection, session_id: Id, student_id: Id, now: NaiveDateTime) -> Result<Status> {
    let s = sessions::get(c, session_id)?;
    let status = domain::status_for_scan(s.starts_at, s.late_after_min, now);
    upsert(c, session_id, student_id, status, Some(now), Source::Badge, None)?;
    Ok(status)
}

/// Correction manuelle (oubli de carte, absence justifiée, ajout d'un non-attendu…).
pub fn set_manual(c: &Connection, session_id: Id, student_id: Id, status: Status, reason: Option<&str>) -> Result<()> {
    let reason = reason.map(str::trim).filter(|r| !r.is_empty());
    let at = matches!(status, Status::Present | Status::Late).then(db::now);
    upsert(c, session_id, student_id, status, at, Source::Manual, reason)
}

/// Supprime la ligne de présence (retour à « non renseigné »).
pub fn clear(c: &Connection, session_id: Id, student_id: Id) -> Result<()> {
    c.execute("DELETE FROM attendance WHERE session_id = ?1 AND student_id = ?2", [session_id, student_id])?;
    Ok(())
}

pub fn open(c: &Connection, session_id: Id) -> Result<()> {
    if sessions::get(c, session_id)?.state == SessionState::Closed {
        return Err(Error::Invalid("Ce créneau est clos : rouvrez-le d'abord".into()));
    }
    sessions::set_state(c, session_id, SessionState::Open)
}

/// Clôture : les attendus sans présence passent « absent » (source auto). Renvoie leur nombre.
pub fn close(c: &Connection, session_id: Id) -> Result<usize> {
    let tx = c.unchecked_transaction()?;
    let n = tx.execute(
        &format!(
            "INSERT INTO attendance(session_id, student_id, status, source, updated_at)
             SELECT ?1, e.id, 'absent', 'auto', ?2 FROM ({}) e
             WHERE e.id NOT IN (SELECT student_id FROM attendance WHERE session_id = ?1)",
            sessions::EXPECTED_SQL
        ),
        params![session_id, db::now()],
    )?;
    sessions::set_state(&tx, session_id, SessionState::Closed)?;
    tx.commit()?;
    Ok(n)
}

/// Réouverture : les absences automatiques sont retirées pour pouvoir badger à nouveau.
pub fn reopen(c: &Connection, session_id: Id) -> Result<()> {
    let tx = c.unchecked_transaction()?;
    tx.execute("DELETE FROM attendance WHERE session_id = ?1 AND source = 'auto'", [session_id])?;
    sessions::set_state(&tx, session_id, SessionState::Open)?;
    tx.commit()?;
    Ok(())
}

/// Créneau ouvert ou clos automatiquement, pour notifier le front.
#[derive(Debug, Clone, Serialize)]
pub struct AutoEvent {
    pub session_id: Id,
    pub title: String,
    pub action: domain::AutoAction,
    /// Absents ajoutés à la clôture.
    pub absents: usize,
}

/// Ouvre / clôt les créneaux dont c'est l'heure. `skip` : créneaux déjà traités (ex. clôturés
/// automatiquement puis rouverts à la main par le prof : on ne les reclôt pas).
pub fn run_auto(
    c: &Connection,
    now: NaiveDateTime,
    open_lead_min: Option<i64>,
    auto_close: bool,
    skip: &std::collections::HashSet<Id>,
) -> Result<Vec<AutoEvent>> {
    let candidates: Vec<Session> = {
        let mut st = c.prepare(
            "SELECT id FROM sessions WHERE state = 'planned' OR (state = 'open' AND ?1) ORDER BY starts_at",
        )?;
        let ids = st.query_map([auto_close], |r| r.get::<_, Id>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter().map(|id| sessions::get(c, id)).collect::<Result<_>>()?
    };
    let mut out = vec![];
    for s in candidates.into_iter().filter(|s| !skip.contains(&s.id)) {
        let Some(action) = domain::auto_action(s.state, s.starts_at, s.ends_at, now, open_lead_min, auto_close) else {
            continue;
        };
        let absents = match action {
            domain::AutoAction::Open => {
                open(c, s.id)?;
                0
            }
            domain::AutoAction::Close => close(c, s.id)?,
        };
        out.push(AutoEvent { session_id: s.id, title: s.title, action, absents });
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{groups, students::tests::input};

    pub fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    /// Promotion BUT2 avec TD1 = {Alice, Bob}, TD2 = {Chloé}. Carte de Alice = 04:A1, Chloé = 04:C3.
    pub fn fixture() -> (Connection, Id, Id, [Id; 3]) {
        let c = crate::db::open_in_memory().unwrap();
        let p = groups::create_program(&c, "BUT Info").unwrap();
        let co = groups::create_cohort(&c, p.id, "BUT2", "2026-2027").unwrap();
        let td1 = groups::create_group(&c, co.id, "TD1").unwrap().id;
        let td2 = groups::create_group(&c, co.id, "TD2").unwrap().id;
        let a = students::create(&c, &input("1", "Dupont", "Alice", Some(co.id))).unwrap().id;
        let b = students::create(&c, &input("2", "Martin", "Bob", Some(co.id))).unwrap().id;
        let ch = students::create(&c, &input("3", "Petit", "Chloé", Some(co.id))).unwrap().id;
        students::set_groups(&c, a, &[td1]).unwrap();
        students::set_groups(&c, b, &[td1]).unwrap();
        students::set_groups(&c, ch, &[td2]).unwrap();
        students::link_card(&c, "04:A1", a).unwrap();
        students::link_card(&c, "04:C3", ch).unwrap();
        let s = sessions::create(
            &c,
            &SessionInput {
                title: "Réseaux".into(),
                room: Some("B12".into()),
                starts_at: dt("2026-11-17 08:00"),
                ends_at: dt("2026-11-17 10:00"),
                late_after_min: 10,
                group_ids: vec![td1],
                cohort_ids: vec![],
            },
        )
        .unwrap();
        (c, s.id, co.id, [a, b, ch])
    }

    #[test]
    fn auto_open_then_close() {
        let (c, sid, _, _) = fixture();
        let none = std::collections::HashSet::new();
        assert!(run_auto(&c, dt("2026-11-17 07:40"), Some(10), true, &none).unwrap().is_empty());
        let ev = run_auto(&c, dt("2026-11-17 07:52"), Some(10), true, &none).unwrap();
        assert_eq!((ev.len(), ev[0].action), (1, domain::AutoAction::Open));
        assert_eq!(sessions::get(&c, sid).unwrap().state, SessionState::Open);
        // Pendant le créneau : rien à faire ; clôture désactivée : rien non plus après la fin.
        assert!(run_auto(&c, dt("2026-11-17 09:00"), Some(10), true, &none).unwrap().is_empty());
        assert!(run_auto(&c, dt("2026-11-17 10:05"), Some(10), false, &none).unwrap().is_empty());
        // Créneau déjà traité (rouvert à la main) : ignoré.
        let skip = std::collections::HashSet::from([sid]);
        assert!(run_auto(&c, dt("2026-11-17 10:05"), Some(10), true, &skip).unwrap().is_empty());
        let ev = run_auto(&c, dt("2026-11-17 10:05"), Some(10), true, &none).unwrap();
        assert_eq!((ev[0].action, ev[0].absents), (domain::AutoAction::Close, 2));
        assert_eq!(sessions::get(&c, sid).unwrap().state, SessionState::Closed);
    }

    #[test]
    fn full_flow() {
        let (c, sid, _, [a, b, ch]) = fixture();
        // Pas encore ouvert
        assert!(matches!(record_scan(&c, sid, "04:A1", dt("2026-11-17 08:00")).unwrap(), ScanOutcome::SessionNotOpen { .. }));
        open(&c, sid).unwrap();
        // Carte inconnue
        assert!(matches!(record_scan(&c, sid, "04:FF", dt("2026-11-17 08:01")).unwrap(), ScanOutcome::UnknownCard { .. }));
        // Alice en retard
        let o = record_scan(&c, sid, "04:A1", dt("2026-11-17 08:15")).unwrap();
        assert!(matches!(o, ScanOutcome::Recorded { status: Status::Late, .. }));
        assert_eq!(o.feedback(), crate::nfc::Feedback::Warn);
        // Deuxième passage
        let o = record_scan(&c, sid, "04:A1", dt("2026-11-17 08:20")).unwrap();
        assert!(matches!(o, ScanOutcome::AlreadyScanned { status: Status::Late, .. }));
        assert_eq!(o.feedback(), crate::nfc::Feedback::Silent); // déjà validé : pas de bip
        // Chloé n'est pas en TD1
        assert!(matches!(record_scan(&c, sid, "04:C3", dt("2026-11-17 08:02")).unwrap(), ScanOutcome::NotExpected { .. }));
        // Ajout manuel de Chloé
        set_manual(&c, sid, ch, Status::Present, Some("oubli de carte")).unwrap();

        let r = sessions::row(&c, sid).unwrap();
        assert_eq!((r.counts.expected, r.counts.present, r.counts.late), (2, 1, 1));

        // Clôture : Bob absent
        assert_eq!(close(&c, sid).unwrap(), 1);
        let d = sessions::detail(&c, sid).unwrap();
        assert_eq!(d.rows.len(), 3);
        let bob = d.rows.iter().find(|r| r.student.id == b).unwrap();
        assert_eq!((bob.status, bob.source), (Some(Status::Absent), Some(Source::Auto)));
        let chloe = d.rows.iter().find(|r| r.student.id == ch).unwrap();
        assert!(!chloe.expected);
        assert_eq!(chloe.reason.as_deref(), Some("oubli de carte"));
        assert!(open(&c, sid).is_err());

        // Réouverture puis Bob arrive : l'absence auto est retirée
        reopen(&c, sid).unwrap();
        students::link_card(&c, "04:B2", b).unwrap();
        assert!(matches!(record_scan(&c, sid, "04:B2", dt("2026-11-17 08:05")).unwrap(), ScanOutcome::Recorded { status: Status::Present, .. }));

        // Justification d'absence
        set_manual(&c, sid, a, Status::Excused, Some("certificat")).unwrap();
        let a_row = sessions::detail(&c, sid).unwrap().rows.into_iter().find(|r| r.student.id == a).unwrap();
        assert_eq!(a_row.status, Some(Status::Excused));
    }

    fn remote(id: &str, uid: &str, at: &str, device_now: &str) -> RemoteScan {
        RemoteScan {
            scan_id: id.into(),
            device: "borne1".into(),
            uid: uid.into(),
            scanned_at: NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M:%S").unwrap(),
            device_now: NaiveDateTime::parse_from_str(device_now, "%Y-%m-%d %H:%M:%S").unwrap(),
        }
    }

    #[test]
    fn remote_scan_live_and_replay() {
        use crate::remote::ReplyKind;
        let (c, sid, _, [a, _, _]) = fixture();
        open(&c, sid).unwrap();
        let now = dt("2026-11-17 08:03");
        let s = remote("borne1-1", "04:A1", "2026-11-17 08:02:55", "2026-11-17 08:03:00");
        let r = record_remote_scan(&c, Some(sid), &s, now).unwrap();
        assert!(matches!(r.outcome, Some(ScanOutcome::Recorded { status: Status::Present, .. })));
        assert_eq!((r.reply.kind, r.reply.display_name.as_deref()), (ReplyKind::Recorded, Some("Alice D.")));
        let expected = sessions::row(&c, sid).unwrap().counts.expected as u32;
        assert_eq!(r.reply.progress, Some(remote::Progress { here: 1, expected }));

        // Rejeu du même scan_id : même réponse, rien de ré-émis, rien de réécrit.
        let again = record_remote_scan(&c, Some(sid), &s, dt("2026-11-17 09:00")).unwrap();
        assert!(again.outcome.is_none());
        assert_eq!(again.reply, r.reply);
        let row = sessions::detail(&c, sid).unwrap().rows.into_iter().find(|x| x.student.id == a).unwrap();
        assert_eq!(row.scanned_at, Some(dt("2026-11-17 08:02")).map(|d| d + chrono::Duration::seconds(55)));

        // UID aléatoire : refusé, mais tracé.
        let rnd = remote("borne1-2", "08:11:22:33", "2026-11-17 08:04:00", "2026-11-17 08:04:00");
        assert!(matches!(record_remote_scan(&c, Some(sid), &rnd, now).unwrap().outcome, Some(ScanOutcome::RandomUid { .. })));
        // Carte inconnue, UID mal formé, scan sans borne.
        let unk = remote("borne1-3", "04:FF", "2026-11-17 08:04:00", "2026-11-17 08:04:00");
        assert!(matches!(record_remote_scan(&c, Some(sid), &unk, now).unwrap().outcome, Some(ScanOutcome::UnknownCard { .. })));
        assert!(record_remote_scan(&c, Some(sid), &remote("borne1-4", "zz", "2026-11-17 08:04:00", "2026-11-17 08:04:00"), now).is_err());
        let mut nodev = unk.clone();
        nodev.scan_id = "x".into();
        nodev.device = " ".into();
        assert!(record_remote_scan(&c, Some(sid), &nodev, now).is_err());
    }

    #[test]
    fn remote_scan_after_close_and_clock_skew() {
        let (c, sid, _, [a, b, _]) = fixture();
        open(&c, sid).unwrap();
        assert_eq!(close(&c, sid).unwrap(), 2); // Alice et Bob passent absents (auto)
        students::link_card(&c, "04:B2", b).unwrap();

        // Borne hors ligne, horloge en retard de 2 h : badge de 08:05 (heure borne 06:05) rejoué à 10:30.
        // L'actif a disparu (clos) : le créneau est retrouvé par l'heure corrigée, l'absence auto est remplacée.
        let late_sync = remote("borne1-9", "04:A1", "2026-11-17 06:05:00", "2026-11-17 08:30:00");
        let r = record_remote_scan(&c, None, &late_sync, dt("2026-11-17 10:30")).unwrap();
        assert!(matches!(r.outcome, Some(ScanOutcome::Recorded { status: Status::Present, .. })), "{:?}", r.outcome);
        let rows = sessions::detail(&c, sid).unwrap().rows;
        let alice = rows.iter().find(|x| x.student.id == a).unwrap();
        assert_eq!((alice.status, alice.source, alice.scanned_at), (Some(Status::Present), Some(Source::Badge), Some(dt("2026-11-17 08:05"))));

        // Un badge hors de toute fenêtre, sans créneau actif : simple identification.
        let far = remote("borne1-10", "04:B2", "2026-11-17 14:00:00", "2026-11-17 14:00:00");
        let r = record_remote_scan(&c, None, &far, dt("2026-11-17 14:00")).unwrap();
        assert!(matches!(r.outcome, Some(ScanOutcome::Identified { student: Some(_), .. })));
        // Bob reste absent : ce badge n'a rien enregistré.
        let rows = sessions::detail(&c, sid).unwrap().rows;
        assert_eq!(rows.iter().find(|x| x.student.id == b).unwrap().status, Some(Status::Absent));
    }

    /// Second créneau en même temps que le TD1 (`fixture`), pour toute la promotion.
    fn amphi(c: &Connection, co: Id) -> Id {
        sessions::create(
            c,
            &SessionInput {
                title: "Amphi".into(),
                room: None,
                starts_at: dt("2026-11-17 08:30"),
                ends_at: dt("2026-11-17 09:30"),
                late_after_min: 10,
                group_ids: vec![],
                cohort_ids: vec![co],
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn remote_scan_picks_session_where_student_is_expected() {
        let (c, sid, co, [a, _, ch]) = fixture();
        let other = amphi(&c, co);
        open(&c, sid).unwrap();
        open(&c, other).unwrap();
        let now = dt("2026-11-17 08:40");
        // Chloé n'est pas dans le TD1 (affiché = sid) : son badge va sur l'amphi où elle est attendue.
        let r = record_remote_scan(&c, Some(sid), &remote("b-1", "04:C3", "2026-11-17 08:40:00", "2026-11-17 08:40:00"), now).unwrap();
        assert!(matches!(r.outcome, Some(ScanOutcome::Recorded { session_id, .. }) if session_id == other));
        assert!(sessions::detail(&c, other).unwrap().rows.iter().any(|x| x.student.id == ch && x.status.is_some()));
        // La borne apprend le créneau, son repère (B : il commence après le TD1) et sa salle.
        let rs = r.reply.session.unwrap();
        assert_eq!((rs.tag, rs.title.as_str(), rs.progress.here), (Some(1), "Amphi", 1));
        // Alice est attendue sur les deux : le créneau affiché gagne.
        record_remote_scan(&c, Some(sid), &remote("b-2", "04:A1", "2026-11-17 08:41:00", "2026-11-17 08:41:00"), now).unwrap();
        assert!(sessions::detail(&c, sid).unwrap().rows.iter().any(|x| x.student.id == a && x.status.is_some()));
        assert!(!sessions::detail(&c, other).unwrap().rows.iter().any(|x| x.student.id == a && x.status.is_some()));
    }

    #[test]
    fn local_scan_goes_to_every_open_session() {
        let (c, sid, co, [a, b, _]) = fixture();
        let other = amphi(&c, co);
        let at = dt("2026-11-17 08:40");
        // Rien d'ouvert : simple identification.
        assert!(matches!(scan(&c, None, "04:A1", at).unwrap(), ScanOutcome::Identified { student: Some(_), .. }));
        assert!(matches!(scan(&c, None, "04:FF", at).unwrap(), ScanOutcome::Identified { student: None, .. }));
        open(&c, sid).unwrap();
        open(&c, other).unwrap();
        // Chloé : amphi, même si l'écran affiche le TD1.
        let o = scan(&c, Some(sid), "04:C3", at).unwrap();
        assert!(matches!(o, ScanOutcome::Recorded { session_id, status: Status::Present, .. } if session_id == other));
        // Alice, attendue aux deux : le TD1 (commencé plus tôt), puis l'amphi au badge suivant.
        assert!(matches!(scan(&c, None, "04:A1", at).unwrap(), ScanOutcome::Recorded { session_id, .. } if session_id == sid));
        assert!(matches!(scan(&c, None, "04:A1", at).unwrap(), ScanOutcome::Recorded { session_id, .. } if session_id == other));
        assert!(matches!(scan(&c, None, "04:A1", at).unwrap(), ScanOutcome::AlreadyScanned { session_id, .. } if session_id == sid));
        assert!(matches!(scan(&c, None, "04:FF", at).unwrap(), ScanOutcome::UnknownCard { .. }));
        // Seul le TD2 reste ouvert : Bob (TD1) n'y est pas attendu.
        students::link_card(&c, "04:B2", b).unwrap();
        close(&c, sid).unwrap();
        let s3 = sessions::create(
            &c,
            &SessionInput {
                title: "TD2".into(),
                room: None,
                starts_at: dt("2026-11-17 08:30"),
                ends_at: dt("2026-11-17 09:30"),
                late_after_min: 10,
                group_ids: vec![groups::find_or_create_group(&c, co, "TD2").unwrap().0],
                cohort_ids: vec![],
            },
        )
        .unwrap();
        close(&c, other).unwrap();
        open(&c, s3.id).unwrap();
        assert!(matches!(scan(&c, None, "04:B2", at).unwrap(), ScanOutcome::NotExpected { .. }));
        // Un badge d'Alice ne touche pas le TD1 clos (seul un rejeu de borne le peut).
        let td1 = sessions::detail(&c, sid).unwrap();
        assert_eq!(td1.rows.iter().find(|x| x.student.id == a).unwrap().status, Some(Status::Late));
        // Créneaux ouverts dans l'ordre des repères.
        assert_eq!(sessions::open_rows(&c).unwrap().iter().map(|r| r.session.id).collect::<Vec<_>>(), vec![s3.id]);
    }

    #[test]
    fn remote_scans_are_purged() {
        let (c, sid, _, _) = fixture();
        open(&c, sid).unwrap();
        let old = remote("old-1", "04:A1", "2026-10-01 08:00:00", "2026-10-01 08:00:00");
        record_remote_scan(&c, Some(sid), &old, dt("2026-10-01 08:00")).unwrap();
        let new = remote("new-1", "04:A1", "2026-11-17 08:00:00", "2026-11-17 08:00:00");
        record_remote_scan(&c, Some(sid), &new, dt("2026-11-17 08:00")).unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM remote_scans", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn cohort_target() {
        let (c, _, co, _) = fixture();
        let s = sessions::create(
            &c,
            &SessionInput {
                title: "Amphi".into(),
                room: None,
                starts_at: dt("2026-11-18 08:00"),
                ends_at: dt("2026-11-18 09:00"),
                late_after_min: 5,
                group_ids: vec![],
                cohort_ids: vec![co],
            },
        )
        .unwrap();
        assert_eq!(sessions::expected_ids(&c, s.id).unwrap().len(), 3);
        let dup = sessions::duplicate(&c, s.id, dt("2026-11-25 08:00")).unwrap();
        assert_eq!(dup.ends_at, dt("2026-11-25 09:00"));
        assert_eq!(sessions::expected_ids(&c, dup.id).unwrap().len(), 3);
    }
}
