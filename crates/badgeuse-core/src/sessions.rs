//! Créneaux d'émargement et leurs cibles (groupes / promotions).

use crate::model::*;
use crate::students::{STUDENT_COLS, group_names, student_from_row};
use crate::{Error, Result, db, groups};
use chrono::NaiveDateTime;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashSet;

const SESSION_COLS: &str = "id, title, room, starts_at, ends_at, late_after_min, state, series_id";

fn session_from_row(r: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get(0)?,
        title: r.get(1)?,
        room: r.get(2)?,
        starts_at: r.get(3)?,
        ends_at: r.get(4)?,
        late_after_min: r.get(5)?,
        state: r.get(6)?,
        series_id: r.get(7)?,
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

fn clean_room(i: &SessionInput) -> Option<&str> {
    i.room.as_deref().map(str::trim).filter(|r| !r.is_empty())
}

fn insert(c: &Connection, i: &SessionInput, series_id: Option<Id>) -> Result<Id> {
    c.execute(
        "INSERT INTO sessions(title, room, starts_at, ends_at, late_after_min, series_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![i.title.trim(), clean_room(i), i.starts_at, i.ends_at, i.late_after_min, series_id],
    )?;
    let id = c.last_insert_rowid();
    write_targets(c, id, i)?;
    Ok(id)
}

pub fn create(c: &Connection, i: &SessionInput) -> Result<Session> {
    validate(i)?;
    let tx = c.unchecked_transaction()?;
    let id = insert(&tx, i, None)?;
    tx.commit()?;
    get(c, id)
}

// ---------- Séries (créneaux répétés) ----------

/// Dates qu'une série créerait à partir du début de `starts_at` (aperçu, rien n'est écrit).
pub fn preview_series(starts_at: NaiveDateTime, rule: &Recurrence) -> Result<SeriesPlan> {
    crate::domain::recurrence_plan(starts_at.date(), rule)
}

/// Crée un créneau par date de la série : mêmes horaires, salle, seuil de retard et cibles que `i`.
pub fn create_series(c: &Connection, i: &SessionInput, rule: &Recurrence) -> Result<Vec<Session>> {
    validate(i)?;
    let plan = preview_series(i.starts_at, rule)?;
    let (time, length) = (i.starts_at.time(), i.ends_at - i.starts_at);
    let tx = c.unchecked_transaction()?;
    let rule_json = serde_json::to_string(rule).map_err(|e| Error::Invalid(e.to_string()))?;
    tx.execute("INSERT INTO session_series(rule_json, created_at) VALUES (?1, ?2)", params![rule_json, db::now()])?;
    let series = tx.last_insert_rowid();
    let mut ids = Vec::with_capacity(plan.dates.len());
    for d in &plan.dates {
        let starts_at = d.and_time(time);
        ids.push(insert(&tx, &SessionInput { starts_at, ends_at: starts_at + length, ..i.clone() }, Some(series))?);
    }
    tx.commit()?;
    ids.into_iter().map(|id| get(c, id)).collect()
}

/// Autres créneaux de la série touchés par `scope` : seulement ceux encore prévus
/// (un créneau ouvert ou clos garde ses horaires et ses présences).
fn series_targets(c: &Connection, s: &Session, scope: SeriesScope) -> Result<Vec<Session>> {
    let Some(series) = s.series_id else { return Ok(vec![]) };
    if scope == SeriesScope::One {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT {SESSION_COLS} FROM sessions WHERE series_id = ?1 AND id <> ?2 AND state = 'planned'
         AND (?3 = 0 OR starts_at > ?4) ORDER BY starts_at"
    );
    let following = scope == SeriesScope::Following;
    Ok(c.prepare(&sql)?
        .query_map(params![series, s.id, following, s.starts_at], session_from_row)?
        .collect::<rusqlite::Result<_>>()?)
}

/// Modifie un créneau et, selon `scope`, les autres créneaux prévus de sa série.
/// Pour eux : mêmes intitulé, salle, seuil, cibles et horaires ; un changement de date du créneau
/// modifié les décale du même nombre de jours. « Ce créneau seulement » le sort de la série.
/// Renvoie le créneau modifié.
pub fn update_scoped(c: &Connection, id: Id, i: &SessionInput, scope: SeriesScope) -> Result<Session> {
    validate(i)?;
    let before = get(c, id)?;
    let others = series_targets(c, &before, scope)?;
    let shift = i.starts_at.date() - before.starts_at.date();
    let (time, length) = (i.starts_at.time(), i.ends_at - i.starts_at);
    let tx = c.unchecked_transaction()?;
    let n = tx.execute(
        "UPDATE sessions SET title=?1, room=?2, starts_at=?3, ends_at=?4, late_after_min=?5 WHERE id=?6",
        params![i.title.trim(), clean_room(i), i.starts_at, i.ends_at, i.late_after_min, id],
    )?;
    if n == 0 {
        return Err(Error::NotFound("Créneau"));
    }
    write_targets(&tx, id, i)?;
    if scope == SeriesScope::One && before.series_id.is_some() {
        tx.execute("UPDATE sessions SET series_id = NULL WHERE id = ?1", [id])?;
    }
    for o in &others {
        let starts_at = (o.starts_at.date() + shift).and_time(time);
        tx.execute(
            "UPDATE sessions SET title=?1, room=?2, starts_at=?3, ends_at=?4, late_after_min=?5 WHERE id=?6",
            params![i.title.trim(), clean_room(i), starts_at, starts_at + length, i.late_after_min, o.id],
        )?;
        write_targets(&tx, o.id, i)?;
    }
    tx.commit()?;
    get(c, id)
}

/// Supprime un créneau et, selon `scope`, les autres créneaux prévus de sa série.
/// Renvoie les ids supprimés.
pub fn delete_scoped(c: &Connection, id: Id, scope: SeriesScope) -> Result<Vec<Id>> {
    let s = get(c, id)?;
    let mut ids = vec![id];
    ids.extend(series_targets(c, &s, scope)?.iter().map(|o| o.id));
    let tx = c.unchecked_transaction()?;
    for i in &ids {
        tx.execute("DELETE FROM sessions WHERE id = ?1", [*i])?;
    }
    tx.execute("DELETE FROM session_series WHERE id NOT IN (SELECT series_id FROM sessions WHERE series_id IS NOT NULL)", [])?;
    tx.commit()?;
    Ok(ids)
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

/// Créneaux ouverts, dans l'ordre de leurs repères A, B, C… (début, puis id) : le même ordre
/// côté appli (`src/lib/sessionTags.ts`) et sur la borne.
pub fn open_rows(c: &Connection) -> Result<Vec<SessionRow>> {
    let ids: Vec<Id> = c
        .prepare("SELECT id FROM sessions WHERE state = 'open' ORDER BY starts_at, id")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    ids.into_iter().map(|id| row(c, id)).collect()
}

/// Prochain créneau prévu plus tard dans la journée (affiché sur la borne en attente).
pub fn next_planned_today(c: &Connection, now: NaiveDateTime) -> Result<Option<Session>> {
    let end_of_day = now.date().and_hms_opt(23, 59, 59).expect("heure valide");
    Ok(c.query_row(
        &format!(
            "SELECT {SESSION_COLS} FROM sessions WHERE state = 'planned' AND starts_at > ?1 AND starts_at <= ?2
             ORDER BY starts_at, id LIMIT 1"
        ),
        params![now, end_of_day],
        session_from_row,
    )
    .optional()?)
}

/// Créneau actif à retenir maintenant (voir `domain::pick_active`).
pub fn resolve_active(c: &Connection, current: Option<Id>, now: NaiveDateTime) -> Result<Option<Id>> {
    let key = |s: &Session| (s.id, s.state, s.starts_at, s.ends_at);
    let cur = match current {
        Some(id) => get(c, id).ok().map(|s| key(&s)),
        None => None,
    };
    let mut st = c.prepare(&format!("SELECT {SESSION_COLS} FROM sessions WHERE state = 'open'"))?;
    let open = st.query_map([], session_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    let open: Vec<_> = open.iter().map(key).collect();
    Ok(crate::domain::pick_active(cur, &open, now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attendance::tests::{dt, fixture};
    use chrono::NaiveDate;

    fn weekdays_rule(until: &str) -> Recurrence {
        Recurrence {
            weekdays: vec![1, 2, 3, 4, 5],
            interval_weeks: 1,
            until: Some(NaiveDate::parse_from_str(until, "%Y-%m-%d").unwrap()),
            count: None,
            skip_holidays: true,
        }
    }

    fn base(c: &Connection, s: Id) -> SessionInput {
        let r = row(c, s).unwrap();
        SessionInput {
            title: "Émargement du matin".into(),
            room: Some("204".into()),
            starts_at: dt("2026-11-30 09:00"),
            ends_at: dt("2026-11-30 09:30"),
            late_after_min: 10,
            group_ids: r.group_ids,
            cohort_ids: r.cohort_ids,
        }
    }

    #[test]
    fn creates_one_session_per_date() {
        let (c, s, ..) = fixture();
        let created = create_series(&c, &base(&c, s), &weekdays_rule("2026-12-18")).unwrap();
        assert_eq!(created.len(), 15);
        let series = created[0].series_id.unwrap();
        assert!(created.iter().all(|x| x.series_id == Some(series) && x.state == SessionState::Planned));
        assert_eq!(created[14].starts_at, dt("2026-12-18 09:00"));
        assert_eq!(created[14].ends_at, dt("2026-12-18 09:30"));
        // Mêmes cibles que le modèle.
        assert_eq!(row(&c, created[3].id).unwrap().group_ids, row(&c, s).unwrap().group_ids);
        // Un créneau isolé n'a pas de série.
        assert_eq!(get(&c, s).unwrap().series_id, None);
    }

    #[test]
    fn update_following_skips_open_and_past() {
        let (c, s, ..) = fixture();
        let ids: Vec<Id> = create_series(&c, &base(&c, s), &weekdays_rule("2026-12-04")).unwrap().iter().map(|x| x.id).collect();
        // Le 1er (lun.) est clos, on modifie le 3e (mer.) et les suivants.
        set_state(&c, ids[0], SessionState::Closed).unwrap();
        let mut i = base(&c, s);
        i.title = "Appel".into();
        i.starts_at = dt("2026-12-02 09:15");
        i.ends_at = dt("2026-12-02 09:45");
        update_scoped(&c, ids[2], &i, SeriesScope::Following).unwrap();
        let all: Vec<Session> = ids.iter().map(|id| get(&c, *id).unwrap()).collect();
        assert_eq!(all[0].title, "Émargement du matin");
        assert_eq!(all[1].starts_at, dt("2026-12-01 09:00"));
        assert_eq!(all[2].starts_at, dt("2026-12-02 09:15"));
        assert_eq!(all[4].title, "Appel");
        assert_eq!(all[4].starts_at, dt("2026-12-04 09:15"));
        assert_eq!(all[4].ends_at, dt("2026-12-04 09:45"));
    }

    #[test]
    fn update_all_shifts_dates_and_one_detaches() {
        let (c, s, ..) = fixture();
        let ids: Vec<Id> = create_series(&c, &base(&c, s), &weekdays_rule("2026-12-04")).unwrap().iter().map(|x| x.id).collect();
        set_state(&c, ids[0], SessionState::Open).unwrap();
        // Toute la série, décalée d'un jour depuis le 2e créneau ; l'ouvert ne bouge pas.
        let mut i = base(&c, s);
        i.starts_at = dt("2026-12-02 10:00");
        i.ends_at = dt("2026-12-02 11:00");
        update_scoped(&c, ids[1], &i, SeriesScope::All).unwrap();
        assert_eq!(get(&c, ids[0]).unwrap().starts_at, dt("2026-11-30 09:00"));
        assert_eq!(get(&c, ids[4]).unwrap().starts_at, dt("2026-12-05 10:00"));
        // Ce créneau seulement : il sort de la série, les autres ne changent pas.
        i.title = "Exception".into();
        let one = update_scoped(&c, ids[3], &i, SeriesScope::One).unwrap();
        assert_eq!(one.series_id, None);
        assert_eq!(get(&c, ids[4]).unwrap().title, "Émargement du matin");
    }

    #[test]
    fn delete_scopes() {
        let (c, s, ..) = fixture();
        let ids: Vec<Id> = create_series(&c, &base(&c, s), &weekdays_rule("2026-12-04")).unwrap().iter().map(|x| x.id).collect();
        set_state(&c, ids[0], SessionState::Closed).unwrap();
        assert_eq!(delete_scoped(&c, ids[1], SeriesScope::One).unwrap(), vec![ids[1]]);
        assert_eq!(delete_scoped(&c, ids[2], SeriesScope::Following).unwrap(), vec![ids[2], ids[3], ids[4]]);
        // Le créneau clos reste, et la série tant qu'il en fait partie.
        assert!(get(&c, ids[0]).is_ok());
        let n: i64 = c.query_row("SELECT COUNT(*) FROM session_series", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        delete_scoped(&c, ids[0], SeriesScope::All).unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM session_series", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
}
