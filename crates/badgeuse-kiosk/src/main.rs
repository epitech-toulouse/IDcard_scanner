//! Borne de scan : lit les badges NFC, les envoie à l'appli Badgeuse (réseau local) et affiche le résultat.
//!
//! Usage : `badgeuse-kiosk [--config /etc/badgeuse-kiosk.toml]` (ou variable `BADGEUSE_KIOSK_CONFIG`).

mod client;
mod config;
mod display;
mod kiosk;
mod queue;
mod readers;
mod updater;
mod web;

use client::HttpClient;
use config::Config;
use kiosk::Kiosk;
use queue::Queue;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Période du signal de vie et du renvoi des badges en attente.
const FLUSH_EVERY: Duration = Duration::from_secs(10);

fn config_path() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--config" && let Some(p) = args.next() {
            return p.into();
        }
    }
    std::env::var_os("BADGEUSE_KIOSK_CONFIG").map(PathBuf::from).unwrap_or_else(|| "/etc/badgeuse-kiosk.toml".into())
}

fn fail(msg: String) -> ! {
    eprintln!("badgeuse-kiosk : {msg}");
    std::process::exit(2)
}

/// Étage root de la mise à jour : `badgeuse-kiosk --apply-update` (lancé par `badgeuse-kiosk-update.service`).
fn apply_update(cfg: &Config) -> ! {
    if cfg.update_public_key.is_empty() {
        fail("update_public_key absente de la configuration : mise à jour refusée".into());
    }
    let private = std::path::Path::new("/run/badgeuse-kiosk-update");
    match updater::apply(&cfg.update_public_key, &cfg.update_dir, private, updater::CURRENT, updater::dpkg_install) {
        Ok(v) => {
            tracing::info!("borne mise à jour en version {v}");
            std::process::exit(0)
        }
        Err(e) => fail(format!("mise à jour abandonnée : {e}")),
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let path = config_path();
    let cfg = Config::load(&path).unwrap_or_else(|e| fail(e));
    if std::env::args().any(|a| a == "--apply-update") {
        apply_update(&cfg);
    }
    let queue = Queue::open(&cfg.queue_path).unwrap_or_else(|e| fail(e));
    let server = web::bind(&cfg.listen).unwrap_or_else(|e| fail(e));
    tracing::info!("borne « {} » → {} · affichage sur http://{}", cfg.device, cfg.server_url, cfg.listen);

    let mut kiosk = Kiosk::new(cfg.device.clone(), queue, Arc::new(HttpClient::new(&cfg)), Box::new(badgeuse_core::db::now));
    if !cfg.update_public_key.is_empty() {
        kiosk = kiosk.with_updates(updater::UpdateConfig { public_key: cfg.update_public_key.clone(), dir: cfg.update_dir.clone() });
    }
    let kiosk = Arc::new(kiosk);
    kiosk.display.set_show_for(Duration::from_secs(cfg.show_secs));
    let mock = readers::start(kiosk.clone(), &cfg);
    web::spawn(server, kiosk.clone(), mock);

    loop {
        kiosk.flush();
        std::thread::sleep(FLUSH_EVERY);
    }
}
