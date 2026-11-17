//! Cursus (program) > promotion (cohort) > groupe (group).

use crate::error::unique_violation;
use crate::model::*;
use crate::{Error, Result};
use rusqlite::{Connection, OptionalExtension, params};

fn clean(name: &str, what: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(Error::Invalid(format!("Le nom {what} est obligatoire")));
    }
    Ok(n.to_string())
}

pub fn create_program(c: &Connection, name: &str) -> Result<Program> {
    let name = clean(name, "du cursus")?;
    c.execute("INSERT INTO programs(name) VALUES (?1)", [&name])
        .map_err(|e| unique_violation(e, "Ce cursus"))?;
    Ok(Program { id: c.last_insert_rowid(), name })
}

pub fn create_cohort(c: &Connection, program_id: Id, name: &str, school_year: &str) -> Result<Cohort> {
    let name = clean(name, "de la promotion")?;
    let school_year = school_year.trim().to_string();
    c.execute(
        "INSERT INTO cohorts(program_id, name, school_year) VALUES (?1, ?2, ?3)",
        params![program_id, name, school_year],
    )
    .map_err(|e| unique_violation(e, "Cette promotion"))?;
    Ok(Cohort { id: c.last_insert_rowid(), program_id, name, school_year })
}

pub fn create_group(c: &Connection, cohort_id: Id, name: &str) -> Result<Group> {
    let name = clean(name, "du groupe")?;
    c.execute("INSERT INTO groups(cohort_id, name) VALUES (?1, ?2)", params![cohort_id, name])
        .map_err(|e| unique_violation(e, "Ce groupe"))?;
    Ok(Group { id: c.last_insert_rowid(), cohort_id, name })
}

/// Renvoie l'id du groupe `name` de la promotion, en le créant si besoin. (bool = créé)
pub fn find_or_create_group(c: &Connection, cohort_id: Id, name: &str) -> Result<(Id, bool)> {
    let name = clean(name, "du groupe")?;
    let found: Option<Id> = c
        .query_row(
            "SELECT id FROM groups WHERE cohort_id = ?1 AND name = ?2 COLLATE NOCASE",
            params![cohort_id, name],
            |r| r.get(0),
        )
        .optional()?;
    match found {
        Some(id) => Ok((id, false)),
        None => Ok((create_group(c, cohort_id, &name)?.id, true)),
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Program,
    Cohort,
    Group,
}

impl Kind {
    fn table(self) -> &'static str {
        match self {
            Kind::Program => "programs",
            Kind::Cohort => "cohorts",
            Kind::Group => "groups",
        }
    }
}

pub fn rename(c: &Connection, kind: Kind, id: Id, name: &str) -> Result<()> {
    let name = clean(name, "")?;
    let n = c
        .execute(&format!("UPDATE {} SET name = ?1 WHERE id = ?2", kind.table()), params![name, id])
        .map_err(|e| unique_violation(e, "Ce nom"))?;
    if n == 0 {
        return Err(Error::NotFound("Élément"));
    }
    Ok(())
}

pub fn set_school_year(c: &Connection, cohort_id: Id, school_year: &str) -> Result<()> {
    c.execute("UPDATE cohorts SET school_year = ?1 WHERE id = ?2", params![school_year.trim(), cohort_id])
        .map_err(|e| unique_violation(e, "Cette promotion"))?;
    Ok(())
}

/// Supprime en cascade (les étudiants d'une promotion supprimée deviennent « sans promotion »).
pub fn delete(c: &Connection, kind: Kind, id: Id) -> Result<()> {
    c.execute(&format!("DELETE FROM {} WHERE id = ?1", kind.table()), [id])?;
    Ok(())
}

pub fn get_cohort(c: &Connection, id: Id) -> Result<Cohort> {
    c.query_row(
        "SELECT id, program_id, name, school_year FROM cohorts WHERE id = ?1",
        [id],
        |r| Ok(Cohort { id: r.get(0)?, program_id: r.get(1)?, name: r.get(2)?, school_year: r.get(3)? }),
    )
    .optional()?
    .ok_or(Error::NotFound("Promotion"))
}

/// Libellé « Cursus – Promotion (année) ».
pub fn cohort_label(c: &Connection, cohort_id: Id) -> Result<String> {
    Ok(c.query_row(
        "SELECT p.name || ' – ' || co.name || CASE WHEN co.school_year <> '' THEN ' (' || co.school_year || ')' ELSE '' END
         FROM cohorts co JOIN programs p ON p.id = co.program_id WHERE co.id = ?1",
        [cohort_id],
        |r| r.get(0),
    )?)
}

/// Libellé « Promotion / Groupe ».
pub fn group_label(c: &Connection, group_id: Id) -> Result<String> {
    Ok(c.query_row(
        "SELECT co.name || ' / ' || g.name FROM groups g JOIN cohorts co ON co.id = g.cohort_id WHERE g.id = ?1",
        [group_id],
        |r| r.get(0),
    )?)
}

/// Arborescence complète, avec le nombre d'étudiants actifs.
pub fn tree(c: &Connection) -> Result<Vec<ProgramNode>> {
    let mut programs: Vec<ProgramNode> = c
        .prepare("SELECT id, name FROM programs ORDER BY name COLLATE NOCASE")?
        .query_map([], |r| Ok(ProgramNode { program: Program { id: r.get(0)?, name: r.get(1)? }, cohorts: vec![] }))?
        .collect::<rusqlite::Result<_>>()?;

    let mut cohorts_stmt = c.prepare(
        "SELECT co.id, co.program_id, co.name, co.school_year,
                (SELECT COUNT(*) FROM students s WHERE s.cohort_id = co.id AND s.active)
         FROM cohorts co WHERE co.program_id = ?1 ORDER BY co.school_year DESC, co.name COLLATE NOCASE",
    )?;
    let mut groups_stmt = c.prepare(
        "SELECT g.id, g.cohort_id, g.name,
                (SELECT COUNT(*) FROM student_groups sg JOIN students s ON s.id = sg.student_id
                 WHERE sg.group_id = g.id AND s.active)
         FROM groups g WHERE g.cohort_id = ?1 ORDER BY g.name COLLATE NOCASE",
    )?;

    for p in &mut programs {
        let cohorts: Vec<(Cohort, i64)> = cohorts_stmt
            .query_map([p.program.id], |r| {
                Ok((
                    Cohort { id: r.get(0)?, program_id: r.get(1)?, name: r.get(2)?, school_year: r.get(3)? },
                    r.get(4)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for (cohort, student_count) in cohorts {
            let groups = groups_stmt
                .query_map([cohort.id], |r| {
                    Ok(GroupNode {
                        group: Group { id: r.get(0)?, cohort_id: r.get(1)?, name: r.get(2)? },
                        student_count: r.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            p.cohorts.push(CohortNode { cohort, student_count, groups });
        }
    }
    Ok(programs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_and_tree() {
        let c = crate::db::open_in_memory().unwrap();
        let p = create_program(&c, "BUT Informatique").unwrap();
        assert!(create_program(&c, "BUT Informatique").is_err());
        let co = create_cohort(&c, p.id, "BUT2", "2026-2027").unwrap();
        let (g1, created) = find_or_create_group(&c, co.id, "TD1").unwrap();
        assert!(created);
        let (g1b, created) = find_or_create_group(&c, co.id, "td1").unwrap();
        assert_eq!((g1, false), (g1b, created));
        let t = tree(&c).unwrap();
        assert_eq!(t[0].cohorts[0].groups.len(), 1);
        assert_eq!(cohort_label(&c, co.id).unwrap(), "BUT Informatique – BUT2 (2026-2027)");
        delete(&c, Kind::Program, p.id).unwrap();
        assert!(tree(&c).unwrap().is_empty());
    }
}
