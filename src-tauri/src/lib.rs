//! Application Tauri : état partagé, thread lecteur NFC, enregistrement des commandes.
//! La logique métier est dans `badgeuse-core` ; ici on ne fait que brancher.

mod commands;

use badgeuse_core::attendance::{self, ScanOutcome};
use badgeuse_core::model::Id;
use badgeuse_core::nfc::{self, CardInfo, Feedback, LoopOptions, MockHandle, MockReader, ReaderKind, ReaderStatus, ReadersStatus};
use badgeuse_core::db::{self, Connection};
use badgeuse_core::{sessions, settings};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};

pub struct AppState {
    db: Arc<Mutex<Connection>>,
    /// Créneau qui reçoit les badges (None = simple identification).
    active_session: Arc<Mutex<Option<Id>>>,
    /// État de chaque lecteur, indexé par son identifiant stable.
    readers: Arc<Mutex<BTreeMap<String, ReaderStatus>>>,
    feedback: Arc<AtomicBool>,
    mock: Option<MockHandle>,
    db_path: PathBuf,
}

impl AppState {
    pub(crate) fn readers_status(&self) -> ReadersStatus {
        ReadersStatus::build(lock(&self.readers).values().cloned().collect())
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn db(&self) -> MutexGuard<'_, Connection> {
        lock(&self.db)
    }
    pub fn active(&self) -> Option<Id> {
        *lock(&self.active_session)
    }
    pub fn set_active(&self, id: Option<Id>) {
        *lock(&self.active_session) = id;
    }
}

/// Traite un UID comme un passage de carte et notifie le front (`card-scanned`).
pub(crate) fn process_uid(app: &AppHandle, db: &Mutex<Connection>, active: Option<Id>, uid: &str) -> Feedback {
    let result = {
        let conn = lock(db);
        match active {
            Some(sid) => attendance::record_scan(&conn, sid, uid, db::now()),
            None => attendance::identify(&conn, uid),
        }
    };
    match result {
        Ok(outcome) => {
            let _ = app.emit("card-scanned", &outcome);
            outcome.feedback()
        }
        Err(e) => {
            tracing::error!("scan {uid} : {e}");
            let _ = app.emit("app-error", e.to_string());
            Feedback::Error
        }
    }
}

fn on_card(app: &AppHandle, state: &AppState, card: &CardInfo) -> Feedback {
    let uid = card.uid_hex();
    tracing::info!("carte {uid} (SAK {:?})", card.sak);
    if card.is_random_uid() {
        let _ = app.emit("card-scanned", &ScanOutcome::RandomUid { uid });
        return Feedback::Error;
    }
    process_uid(app, &state.db, state.active(), &uid)
}

/// Enregistre l'état d'un lecteur et réémet la vue agrégée vers le front.
fn publish_status(app: &AppHandle, st: ReaderStatus) {
    let state = app.state::<AppState>();
    let agg = {
        let mut map = lock(&state.readers);
        map.insert(st.name.clone(), st);
        ReadersStatus::build(map.values().cloned().collect())
    };
    let _ = app.emit("reader-status", &agg);
}

/// Lance une boucle de lecture dans un thread dédié pour un lecteur donné.
fn spawn_worker<F>(app: AppHandle, id: String, open: F)
where
    F: FnMut() -> Result<Box<dyn nfc::CardReader>, nfc::NfcError> + Send + 'static,
{
    std::thread::Builder::new()
        .name(format!("nfc:{id}"))
        .spawn(move || {
            let state = app.state::<AppState>();
            let opts = LoopOptions { id, feedback: state.feedback.clone(), ..Default::default() };
            let status_app = app.clone();
            nfc::run_loop(
                open,
                opts,
                move |st| publish_status(&status_app, st),
                |card| on_card(&app, &state, card),
            );
        })
        .expect("thread lecteur");
}

/// Démarre la lecture NFC : un thread par lecteur, avec prise en compte des
/// lecteurs branchés après le lancement (PC/SC). Plusieurs lecteurs alimentent
/// le même créneau actif.
fn start_readers(app: AppHandle, kind: ReaderKind, mock: Option<MockReader>) {
    match kind {
        ReaderKind::Mock => {
            let mut slot = mock.map(|m| Box::new(m) as Box<dyn nfc::CardReader>);
            spawn_worker(app, "mock".into(), move || {
                slot.take().ok_or_else(|| nfc::NfcError::Backend("simulateur arrêté".into()))
            });
        }
        ReaderKind::Usb => {
            spawn_worker(app, "usb".into(), || nfc::open_hardware(ReaderKind::Usb));
        }
        ReaderKind::Pcsc | ReaderKind::Auto => {
            std::thread::Builder::new()
                .name("nfc-supervisor".into())
                .spawn(move || {
                    let mut running: HashSet<String> = HashSet::new();
                    let mut usb_fallback = false;
                    loop {
                        match nfc::pcsc_reader_names() {
                            Ok(names) => {
                                for n in names.into_iter().filter(|n| nfc::is_contactless(n)) {
                                    if running.insert(n.clone()) {
                                        tracing::info!("nouveau lecteur PC/SC : {n}");
                                        let name = n.clone();
                                        spawn_worker(app.clone(), n, move || nfc::open_pcsc_named(&name));
                                    }
                                }
                            }
                            Err(e) => {
                                // pcscd injoignable : en mode Auto, on bascule une fois sur l'USB direct.
                                if kind == ReaderKind::Auto && !usb_fallback && running.is_empty() {
                                    usb_fallback = true;
                                    running.insert("usb".into());
                                    tracing::info!("PC/SC indisponible ({e}), bascule USB direct");
                                    spawn_worker(app.clone(), "usb".into(), || nfc::open_hardware(ReaderKind::Usb));
                                }
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_secs(3));
                    }
                })
                .expect("thread superviseur NFC");
        }
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,badgeuse=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let db_path = app.path().app_data_dir()?.join("badgeuse.db");
            tracing::info!("base : {}", db_path.display());
            let conn = db::open(&db_path)?;
            let cfg = settings::load(&conn)?;
            let reader_setting = std::env::var("BADGEUSE_READER").unwrap_or(cfg.reader.clone());
            let kind: ReaderKind = reader_setting.parse().unwrap_or_else(|e| {
                tracing::warn!("{e}, lecteur auto");
                ReaderKind::Auto
            });
            let active = sessions::current_open(&conn)?;
            let (mock_reader, mock) = match kind {
                ReaderKind::Mock => {
                    let (r, h) = MockReader::new();
                    (Some(r), Some(h))
                }
                _ => (None, None),
            };
            app.manage(AppState {
                db: Arc::new(Mutex::new(conn)),
                active_session: Arc::new(Mutex::new(active)),
                readers: Arc::new(Mutex::new(BTreeMap::new())),
                feedback: Arc::new(AtomicBool::new(cfg.reader_feedback)),
                mock,
                db_path,
            });
            start_readers(app.handle().clone(), kind, mock_reader);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::tree,
            commands::create_program,
            commands::create_cohort,
            commands::create_group,
            commands::rename_node,
            commands::set_school_year,
            commands::delete_node,
            commands::list_students,
            commands::create_student,
            commands::update_student,
            commands::delete_student,
            commands::link_card,
            commands::unlink_card,
            commands::import_preview,
            commands::import_run,
            commands::list_sessions,
            commands::session_detail,
            commands::create_session,
            commands::update_session,
            commands::duplicate_session,
            commands::delete_session,
            commands::open_session,
            commands::close_session,
            commands::reopen_session,
            commands::get_active_session,
            commands::set_active_session,
            commands::set_attendance,
            commands::clear_attendance,
            commands::accept_unexpected,
            commands::rescan,
            commands::export_session,
            commands::export_period,
            commands::reader_status,
            commands::inject_card,
            commands::get_settings,
            commands::save_settings,
            commands::backup_db,
            commands::restore_db,
            commands::data_path,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Badgeuse");
}
