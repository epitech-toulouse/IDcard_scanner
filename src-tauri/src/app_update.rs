//! Mise à jour de l'appli elle-même (plugin updater de Tauri).
//!
//! Seul trafic vers Internet de l'appli : un `GET` du manifeste `latest.json` de la dernière release GitHub,
//! au démarrage puis une fois par jour, si le réglage `app_update_check` est actif. Aucune donnée n'est envoyée
//! (User-Agent du plugin, sans identifiant). Le paquet téléchargé est vérifié avec la clé publique de
//! `tauri.conf.json` avant installation : `.deb` via pkexec (mot de passe administrateur), AppImage remplacée.

use crate::AppState;
use badgeuse_core::{Error, Result, settings};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

/// Première vérification peu après le démarrage, puis une par jour.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(15);
const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);

/// Une seule installation à la fois.
static INSTALLING: AtomicBool = AtomicBool::new(false);

/// Nouvelle version disponible (événement `app-update`).
#[derive(Debug, Clone, Serialize)]
pub struct AppUpdate {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
}

/// Avancement du téléchargement (événement `app-update-progress`).
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("Mise à jour de l'appli : {e}"))
}

async fn find(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>> {
    app.updater().map_err(err)?.check().await.map_err(err)
}

fn check_enabled(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    settings::load(&state.db()).map(|s| s.app_update_check).unwrap_or(false)
}

/// Vérification en tâche de fond : au démarrage puis chaque jour, si le réglage l'autorise.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_AFTER).await;
        loop {
            if check_enabled(&app) {
                match find(&app).await {
                    Ok(Some(u)) => {
                        tracing::info!("nouvelle version de l'appli : {} (installée : {})", u.version, u.current_version);
                        let _ = app.emit("app-update", AppUpdate { version: u.version, current: u.current_version, notes: u.body });
                    }
                    Ok(None) => tracing::debug!("appli à jour"),
                    // Hors ligne, dépôt inaccessible… : rien à signaler à l'enseignant, on réessaiera demain.
                    Err(e) => tracing::info!("{e}"),
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

/// Vérification à la demande (bouton des Réglages) : `None` si l'appli est à jour.
#[tauri::command]
pub async fn app_update_check(app: AppHandle) -> Result<Option<AppUpdate>> {
    Ok(find(&app).await?.map(|u| AppUpdate { version: u.version, current: u.current_version, notes: u.body }))
}

/// Télécharge, vérifie la signature, installe puis redémarre l'appli.
#[tauri::command]
pub async fn app_update_install(app: AppHandle) -> Result<()> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err(err("installation déjà en cours"));
    }
    let result = install(&app).await;
    INSTALLING.store(false, Ordering::SeqCst);
    result?;
    app.restart()
}

async fn install(app: &AppHandle) -> Result<()> {
    let update = find(app).await?.ok_or_else(|| err("l'appli est déjà à jour"))?;
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit("app-update-progress", Progress { downloaded, total });
            },
            || tracing::info!("mise à jour téléchargée et vérifiée, installation"),
        )
        .await
        .map_err(err)
}
