//! Lecteurs NFC de la borne : un thread par lecteur PC/SC (détection à chaud), ou lecteur simulé.

use crate::config::Config;
use crate::kiosk::Kiosk;
use badgeuse_core::nfc::{self, CardReader, LoopOptions, MockHandle, MockReader, NfcError, ReaderKind};
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Connected = Arc<Mutex<BTreeMap<String, bool>>>;

fn spawn_worker<F>(kiosk: Arc<Kiosk>, cfg: &Config, connected: Connected, id: String, open: F)
where
    F: FnMut() -> Result<Box<dyn CardReader>, NfcError> + Send + 'static,
{
    let opts = LoopOptions {
        id: id.clone(),
        feedback: Arc::new(AtomicBool::new(cfg.feedback)),
        beep: Arc::new(AtomicU8::new(cfg.beep_level().as_u8())),
        // PC/SC réveille le thread dès qu'une carte arrive : le délai n'est qu'un garde-fou, inutile de
        // se réveiller 3 fois par seconde et par lecteur sur un Raspberry Pi.
        poll: Duration::from_secs(2),
        ..Default::default()
    };
    std::thread::Builder::new()
        .name(format!("nfc:{id}"))
        .spawn(move || {
            let on_status = {
                let (kiosk, connected) = (kiosk.clone(), connected.clone());
                move |st: nfc::ReaderStatus| {
                    let mut map = connected.lock().unwrap_or_else(|e| e.into_inner());
                    map.insert(st.name, st.connected);
                    kiosk.display.set_readers(map.values().filter(|c| **c).count());
                }
            };
            nfc::run_loop(open, opts, on_status, |card| kiosk.on_card(card));
        })
        .expect("thread lecteur");
}

/// Lance la lecture. Renvoie la poignée du lecteur simulé (`reader = "mock"`), pour la page d'affichage.
pub fn start(kiosk: Arc<Kiosk>, cfg: &Config) -> Option<MockHandle> {
    let connected: Connected = Arc::new(Mutex::new(BTreeMap::new()));
    if cfg.reader_kind().ok()? == ReaderKind::Mock {
        let (reader, handle) = MockReader::new();
        let mut slot = Some(Box::new(reader) as Box<dyn CardReader>);
        spawn_worker(kiosk, cfg, connected, "mock".into(), move || {
            slot.take().ok_or_else(|| NfcError::Backend("simulateur arrêté".into()))
        });
        return Some(handle);
    }

    // PC/SC : un thread par lecteur sans contact, y compris ceux branchés après le démarrage.
    let cfg = cfg.clone();
    std::thread::Builder::new()
        .name("nfc-supervisor".into())
        .spawn(move || {
            let mut running: HashSet<String> = HashSet::new();
            loop {
                match nfc::pcsc_reader_names() {
                    Ok(names) => {
                        for name in names.into_iter().filter(|n| nfc::is_contactless(n)) {
                            if running.insert(name.clone()) {
                                tracing::info!("lecteur PC/SC : {name}");
                                let open_name = name.clone();
                                spawn_worker(kiosk.clone(), &cfg, connected.clone(), name, move || nfc::open_pcsc_named(&open_name));
                            }
                        }
                    }
                    // pcscd pas encore prêt : l'écran affiche « Lecteur NFC non détecté », on réessaie.
                    Err(e) => tracing::debug!("PC/SC indisponible : {e}"),
                }
                std::thread::sleep(Duration::from_secs(3));
            }
        })
        .expect("thread superviseur NFC");
    None
}
