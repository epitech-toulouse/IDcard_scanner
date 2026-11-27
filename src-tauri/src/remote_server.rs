//! Serveur HTTP local qui reçoit les badges des bornes (Raspberry Pi) sur le réseau local.
//! Désactivé par défaut (réglage `remote_enabled`). Chaque requête doit présenter le jeton
//! (`Authorization: Bearer …`). Aucune donnée ne sort : l'appli ne fait que répondre à la borne.

use crate::{AppState, lock, publish_status};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use badgeuse_core::nfc::{self, CardInfo, ReaderStatus};
use badgeuse_core::remote::{self, HeartbeatReply, RemoteHeartbeat, RemoteReply, RemoteScan};
use badgeuse_core::{Error, attendance, db, sessions, settings, update};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Une borne sans signe de vie depuis ce délai est affichée « déconnectée ».
const STALE_AFTER: Duration = Duration::from_secs(30);

struct Srv {
    app: AppHandle,
    /// Dernier signe de vie (heartbeat ou badge) de chaque borne.
    seen: Mutex<HashMap<String, Instant>>,
}

/// Démarre le serveur dans le runtime async de Tauri (port d'écoute : réglage `remote_port`).
pub fn start(app: AppHandle, port: u16) {
    let srv = Arc::new(Srv { app, seen: Mutex::new(HashMap::new()) });
    let router = Router::new()
        .route("/api/scan", post(scan))
        .route("/api/heartbeat", post(heartbeat))
        .route("/api/update/package", get(update_package))
        .layer(DefaultBodyLimit::max(4096))
        .with_state(srv.clone());

    let watcher = srv.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            watcher.expire_stale();
        }
    });

    tauri::async_runtime::spawn(async move {
        match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
            Ok(listener) => {
                tracing::info!("bornes réseau : écoute sur le port {port}");
                if let Err(e) = axum::serve(listener, router).await {
                    tracing::error!("serveur des bornes arrêté : {e}");
                }
            }
            Err(e) => {
                let msg = format!("Bornes réseau : impossible d'écouter sur le port {port} ({e})");
                tracing::error!("{msg}");
                let _ = srv.app.emit("app-error", msg);
            }
        }
    });
}

impl Srv {
    fn authorized(&self, headers: &HeaderMap) -> bool {
        let Some(given) = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
        else {
            return false;
        };
        let state = self.app.state::<AppState>();
        let expected = settings::load(&state.db()).map(|s| s.remote_token).unwrap_or_default();
        // Jamais de borne acceptée sans jeton configuré.
        !expected.is_empty() && remote::token_matches(&expected, given.trim())
    }

    fn touch(&self, device: &str, pending: Option<u32>, version: Option<&str>) {
        let first = lock(&self.seen).insert(device.to_string(), Instant::now()).is_none();
        let state = self.app.state::<AppState>();
        let known = lock(&state.readers).get(&format!("borne:{device}")).is_some_and(|r| r.connected);
        // On ne ré-émet l'état que s'il change, ou pour rafraîchir le compteur d'attente.
        if first || !known || pending.is_some() {
            let label = match version {
                Some(v) => format!("Borne {device} (v{v})"),
                None => format!("Borne {device}"),
            };
            let message = match pending {
                Some(n) if n > 0 => format!("{label} · {n} badge(s) en attente d'envoi"),
                _ => label,
            };
            publish_status(
                &self.app,
                ReaderStatus { name: format!("borne:{device}"), connected: true, backend: "remote".into(), message },
            );
        }
    }

    fn expire_stale(&self) {
        let stale: Vec<String> = {
            let mut seen = lock(&self.seen);
            let old: Vec<String> = seen.iter().filter(|(_, t)| t.elapsed() > STALE_AFTER).map(|(d, _)| d.clone()).collect();
            for d in &old {
                seen.remove(d);
            }
            old
        };
        for device in stale {
            tracing::warn!("borne {device} : plus de signal");
            publish_status(
                &self.app,
                ReaderStatus {
                    name: format!("borne:{device}"),
                    connected: false,
                    backend: "none".into(),
                    message: format!("Borne {device} : plus de signal"),
                },
            );
        }
    }
}

fn fail(e: Error) -> Response {
    let code = match e {
        // Requête inexploitable (UID mal formé…) : la borne ne doit pas la renvoyer.
        Error::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (code, e.to_string()).into_response()
}

async fn scan(State(srv): State<Arc<Srv>>, headers: HeaderMap, body: Bytes) -> Response {
    if !srv.authorized(&headers) {
        return (StatusCode::UNAUTHORIZED, "Jeton invalide").into_response();
    }
    let scan: RemoteScan = match serde_json::from_slice(&body) {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNPROCESSABLE_ENTITY, format!("Badge illisible : {e}")).into_response(),
    };
    srv.touch(&scan.device, None, None);
    let app = &srv.app;
    let state = app.state::<AppState>();

    // « Test cartes » : un badge en direct est diagnostiqué sans rien enregistrer. Un badge de la
    // file d'attente est refusé (503) pour que la borne le garde et le renvoie plus tard.
    if state.card_test.load(std::sync::atomic::Ordering::Relaxed) {
        if scan.is_replay() {
            return (StatusCode::SERVICE_UNAVAILABLE, "Mode test cartes actif").into_response();
        }
        let Some(bytes) = nfc::parse_uid(&scan.uid) else {
            return fail(Error::Invalid(format!("UID invalide : {}", scan.uid)));
        };
        let card = CardInfo::new(bytes);
        let fb = crate::on_test_card(app, &state, &format!("borne:{}", scan.device), &card);
        let outcome = {
            let conn = state.db();
            attendance::identify(&conn, &card.uid_hex())
        };
        return match outcome {
            Ok(o) => {
                let mut reply = RemoteReply::from_outcome(&o);
                reply.feedback = fb;
                Json(reply).into_response()
            }
            Err(e) => fail(e),
        };
    }

    let result = {
        let conn = state.db();
        attendance::record_remote_scan(&conn, state.active(), &scan, db::now())
    };
    match result {
        Ok(rec) => {
            if let Some(outcome) = &rec.outcome {
                tracing::info!("[borne:{}] carte {} → {:?}", scan.device, scan.uid, rec.reply.kind);
                let _ = app.emit("card-scanned", outcome);
            }
            Json(rec.reply).into_response()
        }
        Err(e) => {
            tracing::error!("[borne:{}] scan {} : {e}", scan.device, scan.scan_id);
            if !matches!(e, Error::Invalid(_)) {
                let _ = app.emit("app-error", e.to_string());
            }
            fail(e)
        }
    }
}

async fn heartbeat(State(srv): State<Arc<Srv>>, headers: HeaderMap, body: Bytes) -> Response {
    if !srv.authorized(&headers) {
        return (StatusCode::UNAUTHORIZED, "Jeton invalide").into_response();
    }
    match serde_json::from_slice::<RemoteHeartbeat>(&body) {
        Ok(hb) if !hb.device.trim().is_empty() => {
            // Version de la borne : texte libre venu du réseau, on ne garde que ce qui ressemble à un numéro.
            let version = hb.version.as_deref().filter(|v| update::valid_version(v));
            srv.touch(&hb.device, Some(hb.pending), version);
            let state = srv.app.state::<AppState>();
            let conn = state.db();
            let open = sessions::open_rows(&conn).unwrap_or_default();
            let next = sessions::next_planned_today(&conn, db::now()).ok().flatten();
            let update = update::published(&conn).ok().flatten();
            drop(conn);
            let reply = HeartbeatReply::new(
                remote::RemoteSession::list(&open),
                next.as_ref().map(remote::NextSession::from_session),
                update,
            );
            Json(reply).into_response()
        }
        _ => (StatusCode::UNPROCESSABLE_ENTITY, "Signal de vie illisible").into_response(),
    }
}

/// Sert le paquet de mise à jour publié. La borne en vérifie elle-même la signature : ce point d'accès
/// n'est qu'un transport (jeton obligatoire comme pour le reste).
async fn update_package(State(srv): State<Arc<Srv>>, headers: HeaderMap) -> Response {
    if !srv.authorized(&headers) {
        return (StatusCode::UNAUTHORIZED, "Jeton invalide").into_response();
    }
    let app = srv.app.clone();
    let read = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let offer = update::published(&state.db()).ok().flatten()?;
        let data = std::fs::read(crate::commands::update_package_path(&state)).ok()?;
        update::verify_package(&offer, &data).ok().map(|()| data)
    })
    .await
    .ok()
    .flatten();
    match read {
        Some(data) => ([(header::CONTENT_TYPE, "application/octet-stream")], data).into_response(),
        None => (StatusCode::NOT_FOUND, "Aucune mise à jour publiée").into_response(),
    }
}
