//! Client HTTP vers l'appli Badgeuse (réseau local, jeton en en-tête).

use crate::config::Config;
use badgeuse_core::remote::{HeartbeatReply, RemoteHeartbeat, RemoteReply, RemoteScan};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Pourquoi un envoi n'a pas abouti — et que faire du badge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    /// Réseau coupé, appli éteinte, mode test… : on garde le badge et on réessaiera.
    Retry(String),
    /// Jeton refusé : on garde le badge (le réglage se corrige), mais l'écran le signale.
    Unauthorized,
    /// Badge inexploitable (refusé par l'appli, 4xx) : inutile de le renvoyer, on l'abandonne.
    Drop(String),
}

/// Ce dont la borne a besoin côté réseau ; une doublure remplace le vrai client dans les tests.
pub trait Transport: Send + Sync {
    fn send_scan(&self, scan: &RemoteScan) -> Result<RemoteReply, SendError>;
    fn heartbeat(&self, pending: u32) -> Result<HeartbeatReply, SendError>;
}

pub struct HttpClient {
    agent: ureq::Agent,
    base: String,
    auth: String,
    device: String,
}

impl HttpClient {
    pub fn new(cfg: &Config) -> HttpClient {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            // Connexion courte : sur le réseau local, un PC éteint ne doit pas bloquer le retour au badge
            // pendant 3 s (le badge reste en file de toute façon).
            .timeout_connect(Some(Duration::from_secs(1)))
            .timeout_global(Some(Duration::from_secs(3)))
            .http_status_as_error(false)
            .build()
            .into();
        HttpClient { agent, base: cfg.server_url.clone(), auth: format!("Bearer {}", cfg.token), device: cfg.device.clone() }
    }

    fn post<B: Serialize, R: DeserializeOwned>(&self, path: &str, body: &B) -> Result<R, SendError> {
        let mut resp = self
            .agent
            .post(format!("{}{path}", self.base))
            .header("Authorization", &self.auth)
            .send_json(body)
            .map_err(|e| SendError::Retry(e.to_string()))?;
        let status = resp.status().as_u16();
        match status {
            200 => resp.body_mut().read_json::<R>().map_err(|e| SendError::Retry(format!("réponse illisible : {e}"))),
            401 => Err(SendError::Unauthorized),
            // Requête refusée pour de bon (UID mal formé…) : le renvoyer ne servirait à rien.
            400 | 413 | 422 => {
                let why = resp.body_mut().read_to_string().unwrap_or_default();
                Err(SendError::Drop(format!("HTTP {status} {why}")))
            }
            // 503 (mode test de l'appli), 5xx, proxy… : on réessaiera.
            _ => Err(SendError::Retry(format!("HTTP {status}"))),
        }
    }
}

impl Transport for HttpClient {
    fn send_scan(&self, scan: &RemoteScan) -> Result<RemoteReply, SendError> {
        self.post("/api/scan", scan)
    }

    fn heartbeat(&self, pending: u32) -> Result<HeartbeatReply, SendError> {
        self.post("/api/heartbeat", &RemoteHeartbeat { device: self.device.clone(), pending })
    }
}
