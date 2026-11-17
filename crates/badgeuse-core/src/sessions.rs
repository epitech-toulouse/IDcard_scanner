//! Créneaux d'émargement et leurs cibles (groupes / promotions).

use crate::model::*;
use crate::students::{STUDENT_COLS, group_names, student_from_row};
use crate::{Error, Result, db, groups};
use chrono::NaiveDateTime;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashSet;

const SESSION_COLS: &str = "id, title, room, starts_at, ends_at, late_after_min, state";

fn session_from_row(r: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get(0)?,
        title: r.get(1)?,
        room: r.get(2)?,
        starts_at: r.get(3)?,
        ends_at: r.get(4)?,
        late_after_min: r.get(5)?,
        state: r.get(6)?,
    })
}

/// Sous-requête : ids des étudiants actifs attendus au créneau `?1`.
pub(crate) const EXPECTED_SQL: &str = "SELECT s.id FROM students s WHERE s.active AND (
       s.id IN (SELECT sg.student_id FROM student_groups sg
                JOIN session_groups x ON x.group_id = sg.group_id WHERE x.session_id = ?1)
    OR s.cohort_id IN (SELECT cohort_id FROM session_cohorts WHERE session_id = ?1))";

fn validate(i: &SessionInput) -> Result<()> {
    if i.title.trim().is_empty() {
        return Err(Error::Invalid("L'intitulé du créneau est obligatoire".into()));
    }
    if i.ends_at <= i.starts_at {
        return Err(Error::Invalid("La fin du créneau doit être après le début".into()));
    }
    if i.late_after_min < 0 {
        return Err(Error::Invalid("Le seuil de retard doit être positif".into()));
    }
    if i.group_ids.is_empty() && i.cohort_ids.is_empty() {
        return Err(Error::Invalid("Choisissez au moins un groupe ou une promotion".into()));
    }
    Ok(())
}

fn write_targets(c: &Connection, id: Id, i: &SessionInput) -> Result<()> {
    c.execute("DELETE FROM session_groups WHERE session_id = ?1", [id])?;
    c.execute("DELETE FROM session_cohorts WHERE session_id = ?1", [id])?;
    for g in &i.group_ids {
        c.execute("INSERT OR IGNORE INTO session_groups VALUES (?1, ?2)", [id, *g])?;
    }
    for co in &i.cohort_ids {
        c.execute("INSERT OR IGNORE INTO session_cohorts VALUES (?1, ?2)", [id, *co])?;
    }
    Ok(())
}

pub fn create(c: &Connection, i: &SessionInput) -> Result<Session> {
    validate(i)?;
    let tx = c.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO sessions(title, room, starts_at, ends_at, late_after_min) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![i.title.trim(), i.room.as_deref().map(str::trim).filter(|r| !r.is_empty()), i.starts_at, i.ends_at, i.late_after_min],
    )?;
    let id = tx.last_insert_rowid();
    write_targets(&tx, id, i)?;
    tx.commit()?;
    get(c, id)
}

pub fn update(c: &Connection, id: Id, i: &SessionInput) -> Result<Session> {
    validate(i)?;
    let tx = c.unchecked_transaction()?;
    let n = tx.execute(
        "UPDATE sessions SET title=?1, room=?2, starts_at=?3, ends_at=?4, late_after_min=?5 WHERE id=?6",
        params![i.title.trim(), i.room.as_deref().map(str::trim).filter(|r| !r.is_empty()), i.starts_at, i.ends_at, i.late_after_min, id],
    )?;
    if n == 0 {
        return Err(Error::NotFound("Créneau"));
    }
    write_targets(&tx, id, i)?;
    tx.commit()?;
    get(c, id)
}

/// Duplique un créneau (mêmes cibles) à une nouvelle date de début, en gardant la durée.
pub fn duplicate(c: &Connection, id: Id, new_start: NaiveDateTime) -> Result<Session> {
    let row = row(c, id)?;
    let s = row.session;
    create(
        c,
        &SessionInput {
            title: s.title,
            room: s.room,
            starts_at: new_start,
            ends_at: new_start + (s.ends_at - s.starts_at),
            late_after_min: s.late_after_min,
            group_ids: row.group_ids,
            cohort_ids: row.cohort_ids,
        },
    )
}

pub fn delete(c: &Connection, id: Id) -> Result<()> {
    c.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
    Ok(())
}

pub fn get(c: &Connection, id: Id) -> Result<Session> {
    c.query_row(&format!("SELECT {SESSION_COLS} FROM sessions WHERE id = ?1"), [id], session_from_row)
        .optional()?
        .ok_or(Error::NotFound("Créneau"))
}

pub fn set_state(c: &Connection, id: Id, state: SessionState) -> Result<()> {
    c.execute("UPDATE sessions SET state = ?1 WHERE id = ?2", params![state, id])?;
    Ok(())
}

pub fn expected_ids(c: &Connection, id: Id) -> Result<HashSet<Id>> {
    Ok(c.prepare(EXPECTED_SQL)?.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

pub fn row(c: &Connection, id: Id) -> Result<SessionRow> {
    let session = get(c, id)?;
    let group_ids: Vec<Id> = c
        .prepare("SELECT group_id FROM session_groups WHERE session_id = ?1")?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let cohort_ids: Vec<Id> = c
        .prepare("SELECT cohort_id FROM session_cohorts WHERE session_id = ?1")?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut labels = vec![];
    for co in &cohort_ids {
        labels.push(groups::cohort_label(c, *co)?);
    }
    for g in &group_ids {
        labels.push(groups::group_label(c, *g)?);
    }
    let mut counts = Counts { expected: expected_ids(c, id)?.len() as i64, ..Default::default() };
    let mut stmt = c.prepare("SELECT status, COUNT(*) FROM attendance WHERE session_id = ?1 GROUP BY status")?;
    for r in stmt.query_map([id], |r| Ok((r.get::<_, Status>(0)?, r.get::<_, i64>(1)?)))? {
        let (st, n) = r?;
        match st {
            Status::Present => counts.present = n,
            Status::Late => counts.late = n,
            Status::Absent => counts.absent = n,
            Status::Excused => counts.excused = n,
        }
    }
    Ok(SessionRow { session, group_ids, cohort_ids, targets: labels.join(" · "), counts })
}

/// Créneaux dont le début est dans [from, to] (bornes optionnelles), plus récents d'abord.
pub fn list(c: &Connection, from: Option<NaiveDateTime>, to: Option<NaiveDateTime>) -> Result<Vec<SessionRow>> {
    let ids: Vec<Id> = c
        .prepare(
            "SELECT id FROM sessions WHERE (?1 IS NULL OR starts_at >= ?1) AND (?2 IS NULL OR starts_at <= ?2)
             ORDER BY starts_at DESC",
        )?
        .query_map(params![from, to], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    ids.into_iter().map(|id| row(c, id)).collect()
}

/// Feuille d'émargement : attendus + éventuels non-attendus ajoutés, triés par nom.
pub fn detail(c: &Connection, id: Id) -> Result<SessionDetail> {
    let session = row(c, id)?;
    let expected = expected_ids(c, id)?;
    let sql = format!(
        "SELECT {STUDENT_COLS}, a.status, a.scanned_at, a.source, a.reason
         FROM students s LEFT JOIN attendance a ON a.student_id = s.id AND a.session_id = ?1
         WHERE s.id IN ({EXPECTED_SQL}) OR a.session_id IS NOT NULL
         ORDER BY s.last_name COLLATE NOCASE, s.first_name COLLATE NOCASE"
    );
    let raw: Vec<(Student, Option<Status>, Option<NaiveDateTime>, Option<Source>, Option<String>)> = c
        .prepare(&sql)?
        .query_map([id], |r| Ok((student_from_row(r, 0)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let rows = raw
        .into_iter()
        .map(|(student, status, scanned_at, source, reason)| {
            Ok(AttendanceRow {
                groups: group_names(c, student.id)?,
                expected: expected.contains(&student.id),
                student,
                status,
                scanned_at,
                source,
                reason,
            })
        })
        .collect::<Result<_>>()?;
    Ok(SessionDetail { session, rows })
}

/// Créneau ouvert le plus proche de maintenant (pour reprendre l'émargement au démarrage).
pub fn current_open(c: &Connection) -> Result<Option<Id>> {
    Ok(c.query_row(
        "SELECT id FROM sessions WHERE state = 'open'
         ORDER BY ABS(julianday(starts_at) - julianday(?1)) LIMIT 1",
        [db::now()],
        |r| r.get(0),
    )
    .optional()?)
}
