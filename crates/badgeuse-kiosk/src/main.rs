//! Borne de scan : lit les badges NFC, les envoie à l'appli Badgeuse (réseau local) et affiche le résultat.
//!
//! Usage : `badgeuse-kiosk [--config /etc/badgeuse-kiosk.toml]` (ou variable `BADGEUSE_KIOSK_CONFIG`).

mod client;
mod config;
mod display;
mod kiosk;
mod queue;
mod readers;
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

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let path = config_path();
    let cfg = Config::load(&path).unwrap_or_else(|e| fail(e));
    let queue = Queue::open(&cfg.queue_path).unwrap_or_else(|e| fail(e));
    let server = web::bind(&cfg.listen).unwrap_or_else(|e| fail(e));
    tracing::info!("borne « {} » → {} · affichage sur http://{}", cfg.device, cfg.server_url, cfg.listen);

    let kiosk = Arc::new(Kiosk::new(
        cfg.device.clone(),
        queue,
        Arc::new(HttpClient::new(&cfg)),
        Box::new(badgeuse_core::db::now),
    ));
    let mock = readers::start(kiosk.clone(), &cfg);
    web::spawn(server, kiosk.clone(), mock);

    loop {
        kiosk.flush();
        std::thread::sleep(FLUSH_EVERY);
    }
}
