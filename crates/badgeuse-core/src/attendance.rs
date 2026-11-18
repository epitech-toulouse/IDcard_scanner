//! Enregistrement des présences : badge, correction manuelle, clôture.

use crate::model::*;
use crate::{Error, Result, db, domain, sessions, students};
use chrono::NaiveDateTime;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

/// Résultat d'un passage de carte, envoyé tel quel au front.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScanOutcome {
    /// Présence enregistrée (present ou late).
    Recorded { uid: String, student: Student, status: Status, scanned_at: NaiveDateTime },
    /// L'étudiant avait déjà badgé sur ce créneau.
    AlreadyScanned { uid: String, student: Student, status: Status },
    /// Carte connue mais étudiant non attendu sur ce créneau (le prof peut l'ajouter à la main).
    NotExpected { uid: String, student: Student },
    /// Carte jamais vue : à associer à un étudiant.
    UnknownCard { uid: String },
    /// Carte reconnue alors qu'aucun créneau n'est ouvert.
    Identified { uid: String, student: Option<Student> },
    /// Le créneau actif n'est pas ouvert.
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
    let uid = uid.to_string();
    let session = sessions::get(c, session_id)?;
    if session.state != SessionState::Open {
        return Ok(ScanOutcome::SessionNotOpen { uid });
    }
    let Some(student) = students::find_by_card(c, &uid)? else {
        return Ok(ScanOutcome::UnknownCard { uid });
    };
    let prev = existing(c, session_id, student.id)?;
    if !domain::scan_overrides(prev) {
        return Ok(ScanOutcome::AlreadyScanned { uid, student, status: prev.unwrap() });
    }
    if prev.is_none() && !sessions::expected_ids(c, session_id)?.contains(&student.id) {
        return Ok(ScanOutcome::NotExpected { uid, student });
    }
    let status = domain::status_for_scan(session.starts_at, session.late_after_min, now);
    upsert(c, session_id, student.id, status, Some(now), Source::Badge, None)?;
    Ok(ScanOutcome::Recorded { uid, student, status, scanned_at: now })
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
