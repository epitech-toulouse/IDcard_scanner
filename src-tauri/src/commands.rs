//! Commandes appelées depuis le front (`src/lib/api.ts`). Rester fin : déléguer à `badgeuse-core`.
//! Côté JS, les noms d'arguments sont en camelCase (`programId`), les champs des structs en snake_case.

use crate::AppState;
use badgeuse_core::attendance::{self, ScanOutcome};
use badgeuse_core::export::{self, ExportSummary, PeriodFilter};
use badgeuse_core::groups::{self, Kind};
use badgeuse_core::import::{self, ColumnMapping, ImportReport, Preview};
use badgeuse_core::model::*;
use badgeuse_core::nfc::{self, ReadersStatus};
use badgeuse_core::settings::{self, Settings};
use badgeuse_core::{Error, Result, db, sessions, students};
use chrono::NaiveDateTime;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};

type S<'a> = State<'a, AppState>;

// ---------- Groupes ----------

#[tauri::command]
pub fn tree(state: S) -> Result<Vec<ProgramNode>> {
    groups::tree(&state.db())
}

#[tauri::command]
pub fn create_program(state: S, name: String) -> Result<Program> {
    groups::create_program(&state.db(), &name)
}

#[tauri::command]
pub fn create_cohort(state: S, program_id: Id, name: String, school_year: String) -> Result<Cohort> {
    groups::create_cohort(&state.db(), program_id, &name, &school_year)
}

#[tauri::command]
pub fn create_group(state: S, cohort_id: Id, name: String) -> Result<Group> {
    groups::create_group(&state.db(), cohort_id, &name)
}

#[tauri::command]
pub fn rename_node(state: S, kind: Kind, id: Id, name: String) -> Result<()> {
    groups::rename(&state.db(), kind, id, &name)
}

#[tauri::command]
pub fn set_school_year(state: S, cohort_id: Id, school_year: String) -> Result<()> {
    groups::set_school_year(&state.db(), cohort_id, &school_year)
}

#[tauri::command]
pub fn delete_node(state: S, kind: Kind, id: Id) -> Result<()> {
    groups::delete(&state.db(), kind, id)
}

// ---------- Étudiants ----------

#[tauri::command]
pub fn list_students(state: S, filter: StudentFilter) -> Result<Vec<StudentRow>> {
    students::list(&state.db(), &filter)
}

#[tauri::command]
pub fn create_student(state: S, input: StudentInput, group_ids: Vec<Id>) -> Result<Student> {
    let c = state.db();
    let s = students::create(&c, &input)?;
    students::set_groups(&c, s.id, &group_ids)?;
    Ok(s)
}

#[tauri::command]
pub fn update_student(state: S, id: Id, input: StudentInput, group_ids: Vec<Id>) -> Result<Student> {
    let c = state.db();
    let s = students::update(&c, id, &input)?;
    students::set_groups(&c, id, &group_ids)?;
    Ok(s)
}

#[tauri::command]
pub fn delete_student(state: S, id: Id) -> Result<()> {
    students::delete(&state.db(), id)
}

#[tauri::command]
pub fn link_card(state: S, uid: String, student_id: Id) -> Result<()> {
    let uid = nfc::parse_uid(&uid).map(|u| nfc::format_uid(&u)).ok_or_else(|| Error::Invalid("UID invalide".into()))?;
    students::link_card(&state.db(), &uid, student_id)
}

#[tauri::command]
pub fn unlink_card(state: S, uid: String) -> Result<()> {
    students::unlink_card(&state.db(), &uid)
}

// ---------- Import ----------

#[tauri::command]
pub fn import_preview(path: PathBuf) -> Result<Preview> {
    import::preview(&path)
}

#[tauri::command]
pub fn import_run(state: S, path: PathBuf, mapping: ColumnMapping, cohort_id: Id) -> Result<ImportReport> {
    let table = import::read_table(&path)?;
    import::import_students(&state.db(), &table, &mapping, cohort_id)
}

// ---------- Créneaux ----------

#[tauri::command]
pub fn list_sessions(state: S, from: Option<NaiveDateTime>, to: Option<NaiveDateTime>) -> Result<Vec<SessionRow>> {
    sessions::list(&state.db(), from, to)
}

#[tauri::command]
pub fn session_detail(state: S, id: Id) -> Result<SessionDetail> {
    sessions::detail(&state.db(), id)
}

#[tauri::command]
pub fn create_session(state: S, input: SessionInput) -> Result<Session> {
    sessions::create(&state.db(), &input)
}

#[tauri::command]
pub fn update_session(state: S, id: Id, input: SessionInput) -> Result<Session> {
    sessions::update(&state.db(), id, &input)
}

#[tauri::command]
pub fn duplicate_session(state: S, id: Id, starts_at: NaiveDateTime) -> Result<Session> {
    sessions::duplicate(&state.db(), id, starts_at)
}

#[tauri::command]
pub fn delete_session(state: S, id: Id) -> Result<()> {
    if state.active() == Some(id) {
        state.set_active(None);
    }
    sessions::delete(&state.db(), id)
}

/// Ouvre le créneau et en fait le créneau actif (celui qui reçoit les badges).
#[tauri::command]
pub fn open_session(state: S, id: Id) -> Result<()> {
    attendance::open(&state.db(), id)?;
    state.set_active(Some(id));
    Ok(())
}

/// Clôt le créneau ; renvoie le nombre d'absents ajoutés automatiquement.
#[tauri::command]
pub fn close_session(state: S, id: Id) -> Result<usize> {
    let n = attendance::close(&state.db(), id)?;
    if state.active() == Some(id) {
        state.set_active(None);
    }
    Ok(n)
}

#[tauri::command]
pub fn reopen_session(state: S, id: Id) -> Result<()> {
    attendance::reopen(&state.db(), id)?;
    state.set_active(Some(id));
    Ok(())
}

#[tauri::command]
pub fn get_active_session(state: S) -> Option<Id> {
    state.active()
}

#[tauri::command]
pub fn set_active_session(state: S, id: Option<Id>) {
    state.set_active(id);
}

#[tauri::command]
pub fn set_attendance(state: S, session_id: Id, student_id: Id, status: Status, reason: Option<String>) -> Result<()> {
    attendance::set_manual(&state.db(), session_id, student_id, status, reason.as_deref())
}

#[tauri::command]
pub fn clear_attendance(state: S, session_id: Id, student_id: Id) -> Result<()> {
    attendance::clear(&state.db(), session_id, student_id)
}

#[tauri::command]
pub fn accept_unexpected(state: S, session_id: Id, student_id: Id) -> Result<Status> {
    attendance::accept_unexpected(&state.db(), session_id, student_id, db::now())
}

/// Rejoue un UID (après association d'une carte inconnue) ; le résultat arrive aussi en événement.
#[tauri::command]
pub fn rescan(app: AppHandle, state: S, uid: String) -> Result<ScanOutcome> {
    let active = state.active();
    let c = state.db();
    let outcome = match active {
        Some(sid) => attendance::record_scan(&c, sid, &uid, db::now())?,
        None => attendance::identify(&c, &uid)?,
    };
    drop(c);
    let _ = tauri::Emitter::emit(&app, "card-scanned", &outcome);
    Ok(outcome)
}

// ---------- Export ----------

#[tauri::command]
pub fn export_session(state: S, session_id: Id, path: PathBuf) -> Result<()> {
    export::export_session(&state.db(), session_id, &path)
}

#[tauri::command]
pub fn export_period(state: S, filter: PeriodFilter, path: PathBuf) -> Result<ExportSummary> {
    export::export_period(&state.db(), &filter, &path)
}

// ---------- Lecteur ----------

#[tauri::command]
pub fn reader_status(state: S) -> ReadersStatus {
    state.readers_status()
}

/// Simule la pose d'une carte (lecteur simulé uniquement).
#[tauri::command]
pub fn inject_card(state: S, uid: String) -> Result<()> {
    let mock = state.mock.as_ref().ok_or_else(|| Error::Invalid("Le lecteur simulé n'est pas actif".into()))?;
    let bytes = nfc::parse_uid(&uid).ok_or_else(|| Error::Invalid("UID invalide (ex. 04:24:A5:82)".into()))?;
    mock.inject(bytes);
    Ok(())
}

// ---------- Réglages & données ----------

#[tauri::command]
pub fn get_settings(state: S) -> Result<Settings> {
    settings::load(&state.db())
}

#[tauri::command]
pub fn save_settings(state: S, settings: Settings) -> Result<()> {
    settings.reader.parse::<nfc::ReaderKind>().map_err(Error::Invalid)?;
    settings::save(&state.db(), &settings)?;
    state.feedback.store(settings.reader_feedback, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn backup_db(state: S, path: PathBuf) -> Result<()> {
    db::backup_to(&state.db(), &path)
}

#[tauri::command]
pub fn restore_db(state: S, path: PathBuf) -> Result<()> {
    let mut c = state.db();
    db::restore_from(&mut c, &path)?;
    let active = sessions::current_open(&c)?;
    drop(c);
    state.set_active(active);
    Ok(())
}

#[tauri::command]
pub fn data_path(state: S) -> String {
    state.db_path.display().to_string()
}
