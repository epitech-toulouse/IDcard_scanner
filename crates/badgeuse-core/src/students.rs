//! Étudiants, appartenance aux groupes et cartes NFC associées.

use crate::error::unique_violation;
use crate::model::*;
use crate::{Error, Result, db, groups};
use rusqlite::{Connection, OptionalExtension, Row, params};

pub(crate) const STUDENT_COLS: &str = "s.id, s.student_number, s.last_name, s.first_name, s.email, s.cohort_id, s.active";

pub(crate) fn student_from_row(r: &Row, offset: usize) -> rusqlite::Result<Student> {
    Ok(Student {
        id: r.get(offset)?,
        student_number: r.get(offset + 1)?,
        last_name: r.get(offset + 2)?,
        first_name: r.get(offset + 3)?,
        email: r.get(offset + 4)?,
        cohort_id: r.get(offset + 5)?,
        active: r.get(offset + 6)?,
    })
}

fn validate(input: &StudentInput) -> Result<()> {
    if input.student_number.trim().is_empty() {
        return Err(Error::Invalid("Le numéro étudiant est obligatoire".into()));
    }
    if input.last_name.trim().is_empty() || input.first_name.trim().is_empty() {
        return Err(Error::Invalid("Le nom et le prénom sont obligatoires".into()));
    }
    Ok(())
}

fn opt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

pub fn create(c: &Connection, input: &StudentInput) -> Result<Student> {
    validate(input)?;
    c.execute(
        "INSERT INTO students(student_number, last_name, first_name, email, cohort_id, active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            input.student_number.trim(),
            input.last_name.trim(),
            input.first_name.trim(),
            opt(&input.email),
            input.cohort_id,
            input.active
        ],
    )
    .map_err(|e| unique_violation(e, "Ce numéro étudiant"))?;
    get(c, c.last_insert_rowid())
}

pub fn update(c: &Connection, id: Id, input: &StudentInput) -> Result<Student> {
    validate(input)?;
    let n = c
        .execute(
            "UPDATE students SET student_number=?1, last_name=?2, first_name=?3, email=?4, cohort_id=?5, active=?6
             WHERE id=?7",
            params![
                input.student_number.trim(),
                input.last_name.trim(),
                input.first_name.trim(),
                opt(&input.email),
                input.cohort_id,
                input.active,
                id
            ],
        )
        .map_err(|e| unique_violation(e, "Ce numéro étudiant"))?;
    if n == 0 {
        return Err(Error::NotFound("Étudiant"));
    }
    get(c, id)
}

/// Crée ou met à jour selon le numéro étudiant. Renvoie (étudiant, créé ?).
pub fn upsert_by_number(c: &Connection, input: &StudentInput) -> Result<(Student, bool)> {
    let existing: Option<Id> = c
        .query_row("SELECT id FROM students WHERE student_number = ?1", [input.student_number.trim()], |r| r.get(0))
        .optional()?;
    match existing {
        Some(id) => {
            // On ne vide pas un email existant si l'import n'en fournit pas.
            let mut input = input.clone();
            if opt(&input.email).is_none() {
                input.email = get(c, id)?.email;
            }
            Ok((update(c, id, &input)?, false))
        }
        None => Ok((create(c, input)?, true)),
    }
}

pub fn delete(c: &Connection, id: Id) -> Result<()> {
    c.execute("DELETE FROM students WHERE id = ?1", [id])?;
    Ok(())
}

pub fn get(c: &Connection, id: Id) -> Result<Student> {
    c.query_row(&format!("SELECT {STUDENT_COLS} FROM students s WHERE s.id = ?1"), [id], |r| student_from_row(r, 0))
        .optional()?
        .ok_or(Error::NotFound("Étudiant"))
}

pub fn set_groups(c: &Connection, student_id: Id, group_ids: &[Id]) -> Result<()> {
    let tx = c.unchecked_transaction()?;
    tx.execute("DELETE FROM student_groups WHERE student_id = ?1", [student_id])?;
    for g in group_ids {
        tx.execute("INSERT INTO student_groups(student_id, group_id) VALUES (?1, ?2)", [student_id, *g])?;
    }
    tx.commit()?;
    Ok(())
}

pub fn add_to_group(c: &Connection, student_id: Id, group_id: Id) -> Result<()> {
    c.execute("INSERT OR IGNORE INTO student_groups(student_id, group_id) VALUES (?1, ?2)", [student_id, group_id])?;
    Ok(())
}

/// Noms des groupes d'un étudiant, triés, séparés par « , ».
pub fn group_names(c: &Connection, student_id: Id) -> Result<String> {
    let names: Vec<String> = c
        .prepare(
            "SELECT g.name FROM student_groups sg JOIN groups g ON g.id = sg.group_id
             WHERE sg.student_id = ?1 ORDER BY g.name COLLATE NOCASE",
        )?
        .query_map([student_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(names.join(", "))
}

pub fn list(c: &Connection, f: &StudentFilter) -> Result<Vec<StudentRow>> {
    let mut sql = format!("SELECT {STUDENT_COLS} FROM students s WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    if !f.include_inactive {
        sql.push_str(" AND s.active");
    }
    if let Some(co) = f.cohort_id {
        sql.push_str(" AND s.cohort_id = ?");
        args.push(Box::new(co));
    }
    if let Some(g) = f.group_id {
        sql.push_str(" AND s.id IN (SELECT student_id FROM student_groups WHERE group_id = ?)");
        args.push(Box::new(g));
    }
    if let Some(q) = f.search.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        sql.push_str(
            " AND (s.last_name LIKE ? OR s.first_name LIKE ? OR s.student_number LIKE ?
               OR (s.first_name || ' ' || s.last_name) LIKE ? OR (s.last_name || ' ' || s.first_name) LIKE ?)",
        );
        let like = format!("%{q}%");
        for _ in 0..5 {
            args.push(Box::new(like.clone()));
        }
    }
    sql.push_str(" ORDER BY s.last_name COLLATE NOCASE, s.first_name COLLATE NOCASE");

    let students: Vec<Student> = c
        .prepare(&sql)?
        .query_map(rusqlite::params_from_iter(args.iter()), |r| student_from_row(r, 0))?
        .collect::<rusqlite::Result<_>>()?;

    let mut gid_stmt = c.prepare("SELECT group_id FROM student_groups WHERE student_id = ?1")?;
    let mut card_stmt = c.prepare("SELECT uid FROM cards WHERE student_id = ?1 ORDER BY linked_at")?;
    students
        .into_iter()
        .map(|s| {
            let group_ids = gid_stmt.query_map([s.id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            let cards = card_stmt.query_map([s.id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            let cohort_label = s.cohort_id.map(|co| groups::cohort_label(c, co)).transpose()?;
            Ok(StudentRow { groups: group_names(c, s.id)?, cohort_label, group_ids, cards, student: s })
        })
        .collect()
}

// ---------- Cartes ----------

pub fn find_by_card(c: &Connection, uid: &str) -> Result<Option<Student>> {
    Ok(c.query_row(
        &format!("SELECT {STUDENT_COLS} FROM cards k JOIN students s ON s.id = k.student_id WHERE k.uid = ?1"),
        [uid],
        |r| student_from_row(r, 0),
    )
    .optional()?)
}

/// Associe une carte à un étudiant. Refuse si la carte appartient déjà à quelqu'un d'autre.
pub fn link_card(c: &Connection, uid: &str, student_id: Id) -> Result<()> {
    if let Some(owner) = find_by_card(c, uid)? {
        if owner.id == student_id {
            return Ok(());
        }
        return Err(Error::Invalid(format!("Cette carte est déjà associée à {}", owner.full_name())));
    }
    c.execute(
        "INSERT INTO cards(uid, student_id, linked_at) VALUES (?1, ?2, ?3)",
        params![uid, student_id, db::now()],
    )?;
    Ok(())
}

pub fn unlink_card(c: &Connection, uid: &str) -> Result<()> {
    c.execute("DELETE FROM cards WHERE uid = ?1", [uid])?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn input(num: &str, last: &str, first: &str, cohort: Option<Id>) -> StudentInput {
        StudentInput {
            student_number: num.into(),
            last_name: last.into(),
            first_name: first.into(),
            email: None,
            cohort_id: cohort,
            active: true,
        }
    }

    #[test]
    fn crud_search_and_cards() {
        let c = crate::db::open_in_memory().unwrap();
        let a = create(&c, &input("123", "Dupont", "Alice", None)).unwrap();
        let b = create(&c, &input("456", "Martin", "Bob", None)).unwrap();
        assert!(create(&c, &input("123", "X", "Y", None)).is_err());

        // Recherche insensible à la casse, sur « prénom nom » comme « nom prénom ».
        for q in ["alice dup", "DUPONT Al", "123"] {
            let f = StudentFilter { search: Some(q.into()), ..Default::default() };
            let r = list(&c, &f).unwrap();
            assert_eq!((r.len(), r[0].student.id), (1, a.id), "{q}");
        }

        link_card(&c, "04:AA", a.id).unwrap();
        link_card(&c, "04:AA", a.id).unwrap(); // idempotent
        assert!(link_card(&c, "04:AA", b.id).is_err());
        assert_eq!(find_by_card(&c, "04:AA").unwrap().unwrap().id, a.id);
        unlink_card(&c, "04:AA").unwrap();
        assert!(find_by_card(&c, "04:AA").unwrap().is_none());
    }
}
