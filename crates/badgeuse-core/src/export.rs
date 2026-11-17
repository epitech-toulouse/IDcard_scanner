//! Exports Excel : feuille d'un créneau, et récapitulatif d'une période avec statistiques.

use crate::domain::Tally;
use crate::model::*;
use crate::{Result, db, sessions};
use chrono::NaiveDateTime;
use rusqlite::Connection;
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook, Worksheet};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

fn status_color(s: Option<Status>) -> Color {
    match s {
        Some(Status::Present) => Color::RGB(0xC6EFCE),
        Some(Status::Late) => Color::RGB(0xFFE699),
        Some(Status::Excused) => Color::RGB(0xBDD7EE),
        Some(Status::Absent) => Color::RGB(0xFFC7CE),
        None => Color::RGB(0xEDEDED),
    }
}

fn header_fmt() -> Format {
    Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x305496))
        .set_font_color(Color::White)
        .set_border(FormatBorder::Thin)
        .set_text_wrap()
        .set_align(FormatAlign::VerticalCenter)
}

fn cell_fmt() -> Format {
    Format::new().set_border(FormatBorder::Thin)
}

fn status_fmt(s: Option<Status>) -> Format {
    cell_fmt().set_background_color(status_color(s)).set_align(FormatAlign::Center)
}

fn title_fmt() -> Format {
    Format::new().set_bold().set_font_size(14)
}

fn fmt_dt(d: NaiveDateTime) -> String {
    d.format("%d/%m/%Y %H:%M").to_string()
}

fn write_headers(ws: &mut Worksheet, row: u32, headers: &[&str], widths: &[f64]) -> Result<()> {
    let h = header_fmt();
    for (i, t) in headers.iter().enumerate() {
        ws.write_string_with_format(row, i as u16, *t, &h)?;
        if let Some(w) = widths.get(i) {
            ws.set_column_width(i as u16, *w)?;
        }
    }
    Ok(())
}

fn write_legend(ws: &mut Worksheet, row: u32) -> Result<()> {
    ws.write_string_with_format(row, 0, "Légende :", &Format::new().set_bold())?;
    let items = [
        (Some(Status::Present), "P = présent"),
        (Some(Status::Late), "R = en retard"),
        (Some(Status::Excused), "AJ = absence justifiée"),
        (Some(Status::Absent), "A = absent"),
        (None, "· = non renseigné"),
    ];
    for (i, (s, t)) in items.iter().enumerate() {
        ws.write_string_with_format(row, i as u16 + 1, *t, &status_fmt(*s))?;
    }
    Ok(())
}

/// Feuille d'émargement d'un créneau.
pub fn export_session(c: &Connection, session_id: Id, path: &Path) -> Result<()> {
    let d = sessions::detail(c, session_id)?;
    let s = &d.session.session;
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Émargement")?;

    ws.write_string_with_format(0, 0, &s.title, &title_fmt())?;
    ws.write_string(
        1,
        0,
        format!(
            "{} – {}{} · Retard après {} min",
            fmt_dt(s.starts_at),
            s.ends_at.format("%H:%M"),
            s.room.as_deref().map(|r| format!(" · Salle {r}")).unwrap_or_default(),
            s.late_after_min
        ),
    )?;
    ws.write_string(2, 0, format!("Groupes : {}", d.session.targets))?;
    let k = &d.session.counts;
    ws.write_string(
        3,
        0,
        format!(
            "Attendus : {} · Présents : {} · Retards : {} · Absents : {} · Justifiés : {}{}",
            k.expected,
            k.present,
            k.late,
            k.absent,
            k.excused,
            if s.state == SessionState::Closed { "" } else { " · (créneau non clos)" }
        ),
    )?;

    let hr = 5;
    write_headers(
        ws,
        hr,
        &["Nom", "Prénom", "N° étudiant", "Groupe(s)", "Statut", "Heure", "Source", "Motif", "Attendu"],
        &[22.0, 18.0, 14.0, 14.0, 18.0, 8.0, 10.0, 30.0, 9.0],
    )?;
    let cf = cell_fmt();
    for (i, r) in d.rows.iter().enumerate() {
        let row = hr + 1 + i as u32;
        ws.write_string_with_format(row, 0, r.student.last_name.to_uppercase(), &cf)?;
        ws.write_string_with_format(row, 1, &r.student.first_name, &cf)?;
        ws.write_string_with_format(row, 2, &r.student.student_number, &cf)?;
        ws.write_string_with_format(row, 3, &r.groups, &cf)?;
        ws.write_string_with_format(row, 4, r.status.map(Status::label).unwrap_or("Non badgé"), &status_fmt(r.status))?;
        ws.write_string_with_format(row, 5, r.scanned_at.map(|t| t.format("%H:%M").to_string()).unwrap_or_default(), &cf)?;
        ws.write_string_with_format(row, 6, r.source.map(Source::label).unwrap_or(""), &cf)?;
        ws.write_string_with_format(row, 7, r.reason.as_deref().unwrap_or(""), &cf)?;
        ws.write_string_with_format(row, 8, if r.expected { "Oui" } else { "Non" }, &cf)?;
    }
    let last = hr + d.rows.len() as u32;
    if !d.rows.is_empty() {
        ws.autofilter(hr, 0, last, 8)?;
    }
    ws.set_freeze_panes(hr + 1, 0)?;
    wb.save(path)?;
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PeriodFilter {
    pub from: Option<NaiveDateTime>,
    pub to: Option<NaiveDateTime>,
    /// Vide + `cohort_ids` vide = tous les étudiants.
    #[serde(default)]
    pub group_ids: Vec<Id>,
    #[serde(default)]
    pub cohort_ids: Vec<Id>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportSummary {
    pub sessions: usize,
    pub students: usize,
}

struct Col {
    session: Session,
    /// Étudiants concernés (attendus ou ayant une ligne de présence) → statut.
    cells: HashMap<Id, Option<Status>>,
}

fn filter_students(c: &Connection, f: &PeriodFilter) -> Result<Option<HashSet<Id>>> {
    if f.group_ids.is_empty() && f.cohort_ids.is_empty() {
        return Ok(None);
    }
    let mut set = HashSet::new();
    let mut by_group = c.prepare("SELECT student_id FROM student_groups WHERE group_id = ?1")?;
    for g in &f.group_ids {
        for id in by_group.query_map([g], |r| r.get(0))? {
            set.insert(id?);
        }
    }
    let mut by_cohort = c.prepare("SELECT id FROM students WHERE cohort_id = ?1")?;
    for co in &f.cohort_ids {
        for id in by_cohort.query_map([co], |r| r.get(0))? {
            set.insert(id?);
        }
    }
    Ok(Some(set))
}

/// Récapitulatif sur une période : matrice étudiants × créneaux + statistiques.
pub fn export_period(c: &Connection, f: &PeriodFilter, path: &Path) -> Result<ExportSummary> {
    let wanted = filter_students(c, f)?;
    let mut list = sessions::list(c, f.from, f.to)?;
    list.reverse(); // ordre chronologique

    let mut cols: Vec<Col> = vec![];
    let mut students: BTreeMap<(String, String, Id), (Student, String)> = BTreeMap::new();
    for sr in list {
        let detail = sessions::detail(c, sr.session.id)?;
        let mut cells = HashMap::new();
        for r in detail.rows {
            if wanted.as_ref().is_some_and(|w| !w.contains(&r.student.id)) {
                continue;
            }
            cells.insert(r.student.id, r.status);
            students
                .entry((r.student.last_name.to_lowercase(), r.student.first_name.to_lowercase(), r.student.id))
                .or_insert((r.student, r.groups));
        }
        if !cells.is_empty() {
            cols.push(Col { session: sr.session, cells });
        }
    }
    let students: Vec<(Student, String)> = students.into_values().collect();

    let mut wb = Workbook::new();
    let cf = cell_fmt();
    let pct = cf.clone().set_num_format("0%");
    let tally_of = |sid: Id| -> Tally {
        let mut t = Tally::default();
        for col in &cols {
            if let Some(st) = col.cells.get(&sid) {
                // Créneau pas encore clos et étudiant non badgé : non compté.
                if st.is_none() && col.session.state != SessionState::Closed {
                    continue;
                }
                t.add(*st);
            }
        }
        t
    };

    // --- Feuille 1 : matrice ---
    {
        let ws = wb.add_worksheet();
        ws.set_name("Présences")?;
        let period = match (f.from, f.to) {
            (Some(a), Some(b)) => format!("du {} au {}", a.format("%d/%m/%Y"), b.format("%d/%m/%Y")),
            (Some(a), None) => format!("depuis le {}", a.format("%d/%m/%Y")),
            (None, Some(b)) => format!("jusqu'au {}", b.format("%d/%m/%Y")),
            (None, None) => "toutes dates".into(),
        };
        ws.write_string_with_format(0, 0, format!("Présences {period}"), &title_fmt())?;
        ws.write_string(1, 0, format!("Généré le {}", fmt_dt(db::now())))?;
        let hr = 3;
        write_headers(ws, hr, &["Nom", "Prénom", "N° étudiant", "Groupe(s)"], &[22.0, 18.0, 14.0, 14.0])?;
        let h = header_fmt().set_align(FormatAlign::Center);
        for (j, col) in cols.iter().enumerate() {
            let cidx = 4 + j as u16;
            ws.write_string_with_format(
                hr,
                cidx,
                format!("{}\n{}", col.session.starts_at.format("%d/%m %Hh%M"), col.session.title),
                &h,
            )?;
            ws.set_column_width(cidx, 11.0)?;
        }
        let rate_col = 4 + cols.len() as u16;
        ws.write_string_with_format(hr, rate_col, "Taux de présence", &h)?;
        ws.set_column_width(rate_col, 11.0)?;
        ws.set_row_height(hr, 45)?;

        for (i, (s, groups)) in students.iter().enumerate() {
            let row = hr + 1 + i as u32;
            ws.write_string_with_format(row, 0, s.last_name.to_uppercase(), &cf)?;
            ws.write_string_with_format(row, 1, &s.first_name, &cf)?;
            ws.write_string_with_format(row, 2, &s.student_number, &cf)?;
            ws.write_string_with_format(row, 3, groups, &cf)?;
            for (j, col) in cols.iter().enumerate() {
                let cidx = 4 + j as u16;
                match col.cells.get(&s.id) {
                    Some(st) => ws.write_string_with_format(row, cidx, st.map(Status::code).unwrap_or("·"), &status_fmt(*st))?,
                    None => ws.write_string_with_format(row, cidx, "", &cf)?,
                };
            }
            ws.write_number_with_format(row, rate_col, tally_of(s.id).attendance_rate(), &pct)?;
        }
        ws.set_freeze_panes(hr + 1, 4)?;
        write_legend(ws, hr + 2 + students.len() as u32)?;
    }

    // --- Feuille 2 : statistiques ---
    {
        let ws = wb.add_worksheet();
        ws.set_name("Statistiques")?;
        ws.write_string_with_format(0, 0, "Par étudiant", &title_fmt())?;
        let heads = ["Nom", "Prénom", "N° étudiant", "Groupe(s)", "Créneaux", "Présent", "Retard", "Abs. justifiée", "Abs. non justifiée", "Taux de présence"];
        write_headers(ws, 1, &heads, &[22.0, 18.0, 14.0, 14.0, 10.0, 10.0, 10.0, 12.0, 12.0, 12.0])?;
        let write_tally = |ws: &mut Worksheet, row: u32, col: u16, t: &Tally| -> Result<()> {
            for (k, v) in [t.sessions, t.present, t.late, t.excused, t.absent].iter().enumerate() {
                ws.write_number_with_format(row, col + k as u16, *v as f64, &cf)?;
            }
            ws.write_number_with_format(row, col + 5, t.attendance_rate(), &pct)?;
            Ok(())
        };

        let mut per_group: BTreeMap<String, (usize, Tally)> = BTreeMap::new();
        for (i, (s, groups)) in students.iter().enumerate() {
            let row = 2 + i as u32;
            let t = tally_of(s.id);
            ws.write_string_with_format(row, 0, s.last_name.to_uppercase(), &cf)?;
            ws.write_string_with_format(row, 1, &s.first_name, &cf)?;
            ws.write_string_with_format(row, 2, &s.student_number, &cf)?;
            ws.write_string_with_format(row, 3, groups, &cf)?;
            write_tally(ws, row, 4, &t)?;
            let names: Vec<&str> = if groups.is_empty() { vec!["(sans groupe)"] } else { groups.split(", ").collect() };
            for g in names {
                let e = per_group.entry(g.to_string()).or_default();
                e.0 += 1;
                e.1.merge(&t);
            }
        }

        let start = 4 + students.len() as u32;
        ws.write_string_with_format(start, 0, "Par groupe", &title_fmt())?;
        write_headers(
            ws,
            start + 1,
            &["Groupe", "Étudiants", "", "", "Créneaux × étudiants", "Présent", "Retard", "Abs. justifiée", "Abs. non justifiée", "Taux de présence"],
            &[],
        )?;
        for (i, (g, (n, t))) in per_group.iter().enumerate() {
            let row = start + 2 + i as u32;
            ws.write_string_with_format(row, 0, g, &cf)?;
            ws.write_number_with_format(row, 1, *n as f64, &cf)?;
            write_tally(ws, row, 4, t)?;
        }
        ws.set_freeze_panes(2, 0)?;
    }

    wb.save(path)?;
    Ok(ExportSummary { sessions: cols.len(), students: students.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attendance::{self, tests::{dt, fixture}};
    use calamine::{Data, Reader};

    #[test]
    fn xlsx_roundtrip() {
        let (c, sid, _, [_, _, chloe]) = fixture();
        attendance::open(&c, sid).unwrap();
        attendance::record_scan(&c, sid, "04:A1", dt("2026-11-17 08:20")).unwrap();
        attendance::set_manual(&c, sid, chloe, Status::Present, Some("ajout")).unwrap();
        attendance::close(&c, sid).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("creneau.xlsx");
        export_session(&c, sid, &p1).unwrap();
        let mut wb = calamine::open_workbook_auto(&p1).unwrap();
        let r = wb.worksheet_range("Émargement").unwrap();
        assert_eq!(r.get_value((0, 0)), Some(&Data::String("Réseaux".into())));
        // En-têtes ligne 6, puis Dupont (retard), Martin (absent), Petit (ajout manuel)
        assert_eq!(r.get_value((6, 4)), Some(&Data::String("En retard".into())));
        assert_eq!(r.get_value((7, 4)), Some(&Data::String("Absent".into())));
        assert_eq!(r.get_value((8, 6)), Some(&Data::String("Manuel".into())));

        let p2 = dir.path().join("periode.xlsx");
        let sum = export_period(&c, &PeriodFilter::default(), &p2).unwrap();
        assert_eq!((sum.sessions, sum.students), (1, 3));
        let mut wb = calamine::open_workbook_auto(&p2).unwrap();
        let m = wb.worksheet_range("Présences").unwrap();
        assert_eq!(m.get_value((4, 4)), Some(&Data::String("R".into())));
        assert_eq!(m.get_value((5, 4)), Some(&Data::String("A".into())));
        assert_eq!(m.get_value((4, 5)), Some(&Data::Float(1.0)));
        let st = wb.worksheet_range("Statistiques").unwrap();
        assert_eq!(st.get_value((3, 8)), Some(&Data::Float(1.0))); // Martin : 1 absence non justifiée
    }
}
