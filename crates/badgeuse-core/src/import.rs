//! Import d'une liste d'étudiants depuis un CSV (UTF-8 ou Windows-1252, `;` `,` ou tabulation)
//! ou un classeur (xlsx, xls, ods — première feuille).

use crate::model::{Id, StudentInput};
use crate::{Error, Result, groups, students};
use calamine::{Data, Reader};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Index (0-based) des colonnes du fichier pour chaque champ.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ColumnMapping {
    pub student_number: Option<usize>,
    pub last_name: Option<usize>,
    pub first_name: Option<usize>,
    pub email: Option<usize>,
    pub group: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub headers: Vec<String>,
    pub sample: Vec<Vec<String>>,
    pub total_rows: usize,
    pub mapping: ColumnMapping,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportReport {
    pub created: usize,
    pub updated: usize,
    pub groups_created: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

pub fn read_table(path: &Path) -> Result<Table> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut all = match ext.as_str() {
        "csv" | "txt" | "tsv" => read_csv(&std::fs::read(path)?)?,
        "xlsx" | "xlsm" | "xls" | "ods" => read_workbook(path)?,
        _ => return Err(Error::Invalid(format!("Format non pris en charge : .{ext} (CSV, XLSX, XLS ou ODS)"))),
    };
    all.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    if all.is_empty() {
        return Err(Error::Invalid("Le fichier est vide".into()));
    }
    let headers = all.remove(0).into_iter().map(|h| h.trim().to_string()).collect();
    Ok(Table { headers, rows: all })
}

fn read_csv(bytes: &[u8]) -> Result<Vec<Vec<String>>> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    };
    let first = text.lines().next().unwrap_or("");
    let delim = [b';', b',', b'\t'].into_iter().max_by_key(|d| first.matches(*d as char).count()).unwrap();
    let mut rdr = csv::ReaderBuilder::new().delimiter(delim).has_headers(false).flexible(true).from_reader(text.as_bytes());
    let mut rows = vec![];
    for rec in rdr.records() {
        rows.push(rec?.iter().map(String::from).collect());
    }
    Ok(rows)
}

fn cell(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        // Les numéros étudiants arrivent souvent en nombre flottant.
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        other => other.to_string(),
    }
}

fn read_workbook(path: &Path) -> Result<Vec<Vec<String>>> {
    let mut wb = calamine::open_workbook_auto(path)?;
    let range = wb.worksheet_range_at(0).ok_or_else(|| Error::Invalid("Classeur sans feuille".into()))??;
    Ok(range.rows().map(|r| r.iter().map(cell).collect()).collect())
}

fn normalize(h: &str) -> String {
    h.to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'à' | 'â' => 'a',
            'î' | 'ï' => 'i',
            'ô' => 'o',
            'û' | 'ù' => 'u',
            'ç' => 'c',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Devine les colonnes à partir des en-têtes usuels des exports de scolarité.
pub fn guess_mapping(headers: &[String]) -> ColumnMapping {
    let mut m = ColumnMapping::default();
    for (i, raw) in headers.iter().enumerate() {
        let h = normalize(raw);
        let words: Vec<&str> = h.split(' ').collect();
        let has = |w: &str| words.contains(&w);
        if m.email.is_none() && (h.contains("mail") || h.contains("courriel")) {
            m.email = Some(i);
        } else if m.first_name.is_none() && (h.contains("prenom") || h.contains("first")) {
            m.first_name = Some(i);
        } else if m.student_number.is_none()
            && (h.contains("numero") || has("num") || has("no") || has("n") || h.contains("matricule")
                || h.contains("identifiant") || has("id") || h.contains("student id") || h.contains("code etudiant"))
        {
            m.student_number = Some(i);
        } else if m.last_name.is_none() && (has("nom") || h.contains("last") || h.contains("surname") || has("name")) {
            m.last_name = Some(i);
        } else if m.group.is_none() && (h.contains("groupe") || h.contains("group") || has("td") || has("tp")) {
            m.group = Some(i);
        }
    }
    m
}

pub fn preview(path: &Path) -> Result<Preview> {
    let t = read_table(path)?;
    Ok(Preview {
        mapping: guess_mapping(&t.headers),
        total_rows: t.rows.len(),
        sample: t.rows.iter().take(8).cloned().collect(),
        headers: t.headers,
    })
}

/// Importe (ou met à jour, clé = n° étudiant) dans la promotion `cohort_id`.
/// Les groupes de la colonne « groupe » (séparés par , ; / ou |) sont créés au besoin
/// et ajoutés à l'étudiant (les appartenances existantes sont conservées).
pub fn import_students(c: &Connection, table: &Table, m: &ColumnMapping, cohort_id: Id) -> Result<ImportReport> {
    groups::get_cohort(c, cohort_id)?;
    let (Some(num_i), Some(last_i), Some(first_i)) = (m.student_number, m.last_name, m.first_name) else {
        return Err(Error::Invalid("Indiquez les colonnes numéro étudiant, nom et prénom".into()));
    };
    let get = |row: &Vec<String>, i: Option<usize>| -> String {
        i.and_then(|i| row.get(i)).map(|s| s.trim().to_string()).unwrap_or_default()
    };

    let tx = c.unchecked_transaction()?;
    let mut rep = ImportReport::default();
    for (n, row) in table.rows.iter().enumerate() {
        let line = n + 2; // ligne 1 = en-têtes
        let input = StudentInput {
            student_number: get(row, Some(num_i)),
            last_name: get(row, Some(last_i)),
            first_name: get(row, Some(first_i)),
            email: Some(get(row, m.email)).filter(|e| !e.is_empty()),
            cohort_id: Some(cohort_id),
            active: true,
        };
        if input.student_number.is_empty() && input.last_name.is_empty() {
            rep.skipped += 1;
            continue;
        }
        let student = match students::upsert_by_number(&tx, &input) {
            Ok((s, created)) => {
                if created { rep.created += 1 } else { rep.updated += 1 }
                s
            }
            Err(e) => {
                rep.errors.push(format!("Ligne {line} : {e}"));
                continue;
            }
        };
        let groups_cell = get(row, m.group);
        for g in groups_cell.split([',', ';', '/', '|']).map(str::trim).filter(|g| !g.is_empty()) {
            let (gid, created) = groups::find_or_create_group(&tx, cohort_id, g)?;
            rep.groups_created += created as usize;
            students::add_to_group(&tx, student.id, gid)?;
        }
    }
    tx.commit()?;
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::StudentFilter;

    #[test]
    fn guess_french_headers() {
        let h: Vec<String> = ["N° étudiant", "NOM", "Prénom", "Courriel", "Groupe TD"].map(String::from).into();
        let m = guess_mapping(&h);
        assert_eq!(
            m,
            ColumnMapping { student_number: Some(0), last_name: Some(1), first_name: Some(2), email: Some(3), group: Some(4) }
        );
    }

    #[test]
    fn csv_cp1252_semicolon_and_idempotent_import() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("liste.csv");
        // « Chloé » encodé en Windows-1252 (é = 0xE9)
        let mut bytes = b"Numero;Nom;Prenom;Groupe\n1;Dupont;Alice;TD1\n2;Martin;Bob;TD1, TP1\n3;Petit;Chlo".to_vec();
        bytes.extend_from_slice(b"\xE9;TD2\n;;;\n");
        std::fs::write(&p, &bytes).unwrap();

        let pv = preview(&p).unwrap();
        assert_eq!(pv.total_rows, 3);
        assert_eq!(pv.sample[2][2], "Chloé");

        let c = crate::db::open_in_memory().unwrap();
        let prog = groups::create_program(&c, "BUT").unwrap();
        let co = groups::create_cohort(&c, prog.id, "BUT1", "2026-2027").unwrap();
        let t = read_table(&p).unwrap();
        let r = import_students(&c, &t, &pv.mapping, co.id).unwrap();
        assert_eq!((r.created, r.updated, r.groups_created), (3, 0, 3));
        let r = import_students(&c, &t, &pv.mapping, co.id).unwrap();
        assert_eq!((r.created, r.updated, r.groups_created), (0, 3, 0));
        let bob = students::list(&c, &StudentFilter { search: Some("Bob".into()), ..Default::default() }).unwrap();
        assert_eq!(bob[0].groups, "TD1, TP1");
    }

    #[test]
    fn missing_required_columns() {
        let c = crate::db::open_in_memory().unwrap();
        let prog = groups::create_program(&c, "BUT").unwrap();
        let co = groups::create_cohort(&c, prog.id, "BUT1", "").unwrap();
        let t = Table { headers: vec!["x".into()], rows: vec![vec!["1".into()]] };
        assert!(import_students(&c, &t, &ColumnMapping::default(), co.id).is_err());
    }
}
