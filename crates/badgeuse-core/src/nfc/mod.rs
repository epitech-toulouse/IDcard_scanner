//! Lecteurs NFC : abstraction commune, sélection automatique et boucle de lecture.
//!
//! Backends :
//! - `pcsc` (feature `pcsc`) : standard, via pcscd. Nécessite que `pn533_usb` soit blacklisté.
//! - `usb`  (feature `usb`)  : accès direct libusb à un ACR122U (détache le pilote noyau).
//! - `mock` : lecteur simulé alimenté par [`MockHandle::inject`], pour le dev et les tests.

#[allow(dead_code)]
pub(crate) mod acr122;
pub mod mock;
#[cfg(feature = "pcsc")]
pub mod pcsc;
#[cfg(feature = "usb")]
pub mod usb;

pub use mock::{MockHandle, MockReader};

use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardInfo {
    pub uid: Vec<u8>,
    /// Octet SAK si le backend le connaît (USB direct).
    pub sak: Option<u8>,
}

impl CardInfo {
    pub fn new(uid: Vec<u8>) -> Self {
        CardInfo { uid, sak: None }
    }
    /// Format canonique stocké en base : « 04:24:A5:82:E6:19:90 ».
    pub fn uid_hex(&self) -> String {
        format_uid(&self.uid)
    }
    /// UID aléatoire ISO 14443-3 : 4 octets commençant par 0x08 (téléphones, cartes bancaires…).
    pub fn is_random_uid(&self) -> bool {
        self.uid.len() == 4 && self.uid[0] == 0x08
    }
}

pub fn format_uid(uid: &[u8]) -> String {
    uid.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

/// Normalise une saisie « 04 24 a5… », « 0424A5… » ou « 04:24:A5… ».
pub fn parse_uid(s: &str) -> Option<Vec<u8>> {
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.is_empty() || hex.len() % 2 != 0 || s.chars().any(|c| !c.is_ascii_hexdigit() && !" :-".contains(c)) {
        return None;
    }
    (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Feedback {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, thiserror::Error)]
pub enum NfcError {
    #[error("Aucun lecteur détecté : {0}")]
    NoReader(String),
    #[error("{0}")]
    Backend(String),
}

pub trait CardReader: Send {
    /// Nom affiché (« PC/SC : ACS ACR122U 00 00 », « USB : ACR122U214 »…).
    fn name(&self) -> String;
    /// Attend la *pose* d'une carte au plus `timeout`. Une carte laissée sur le lecteur
    /// n'est renvoyée qu'une fois.
    fn wait_card(&mut self, timeout: Duration) -> Result<Option<CardInfo>, NfcError>;
    /// Retour LED/bip, au mieux.
    fn feedback(&mut self, _fb: Feedback) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderKind {
    Auto,
    Pcsc,
    Usb,
    Mock,
}

impl std::str::FromStr for ReaderKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Ok(ReaderKind::Auto),
            "pcsc" => Ok(ReaderKind::Pcsc),
            "usb" => Ok(ReaderKind::Usb),
            "mock" => Ok(ReaderKind::Mock),
            o => Err(format!("type de lecteur inconnu : {o}")),
        }
    }
}

/// Ouvre un lecteur réel. `Auto` essaie PC/SC puis USB direct.
pub fn open_hardware(kind: ReaderKind) -> Result<Box<dyn CardReader>, NfcError> {
    let mut errors: Vec<String> = vec![];
    if matches!(kind, ReaderKind::Auto | ReaderKind::Pcsc) {
        #[cfg(feature = "pcsc")]
        match pcsc::PcscReader::open() {
            Ok(r) => return Ok(Box::new(r)),
            Err(e) => errors.push(format!("PC/SC : {e}")),
        }
        #[cfg(not(feature = "pcsc"))]
        errors.push("PC/SC : non compilé".into());
    }
    if matches!(kind, ReaderKind::Auto | ReaderKind::Usb) {
        #[cfg(feature = "usb")]
        match usb::UsbReader::open() {
            Ok(r) => return Ok(Box::new(r)),
            Err(e) => errors.push(format!("USB : {e}")),
        }
        #[cfg(not(feature = "usb"))]
        errors.push("USB : non compilé".into());
    }
    if kind == ReaderKind::Mock {
        errors.push("le lecteur simulé doit être fourni par l'appelant".into());
    }
    Err(NfcError::NoReader(errors.join(" ; ")))
}

#[derive(Debug, Clone, Serialize)]
pub struct ReaderStatus {
    pub connected: bool,
    /// pcsc | usb | mock | none
    pub backend: String,
    pub message: String,
}

pub struct LoopOptions {
    pub debounce: Duration,
    pub poll: Duration,
    pub retry: Duration,
    pub feedback: Arc<AtomicBool>,
    pub stop: Arc<AtomicBool>,
}

impl Default for LoopOptions {
    fn default() -> Self {
        LoopOptions {
            debounce: Duration::from_secs(3),
            poll: Duration::from_millis(300),
            retry: Duration::from_secs(2),
            feedback: Arc::new(AtomicBool::new(true)),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn backend_of(name: &str) -> &'static str {
    if name.starts_with("PC/SC") {
        "pcsc"
    } else if name.starts_with("USB") {
        "usb"
    } else if name.starts_with("Simul") {
        "mock"
    } else {
        "none"
    }
}

/// Boucle bloquante (à lancer dans un thread) : ouvre le lecteur via `open`, le rouvre en cas
/// d'erreur, ignore une même carte pendant `debounce`, et appelle `on_card` pour chaque passage.
pub fn run_loop(
    mut open: impl FnMut() -> Result<Box<dyn CardReader>, NfcError>,
    opts: LoopOptions,
    mut on_status: impl FnMut(ReaderStatus),
    mut on_card: impl FnMut(&CardInfo) -> Feedback,
) {
    let mut last: Option<(Vec<u8>, Instant)> = None;
    let mut last_error = String::new();
    while !opts.stop.load(Ordering::Relaxed) {
        let mut reader = match open() {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                if msg != last_error {
                    tracing::warn!("{msg}");
                    on_status(ReaderStatus { connected: false, backend: "none".into(), message: msg.clone() });
                    last_error = msg;
                }
                std::thread::sleep(opts.retry);
                continue;
            }
        };
        last_error.clear();
        let name = reader.name();
        tracing::info!("lecteur connecté : {name}");
        on_status(ReaderStatus { connected: true, backend: backend_of(&name).into(), message: name });

        while !opts.stop.load(Ordering::Relaxed) {
            match reader.wait_card(opts.poll) {
                Ok(None) => {}
                Ok(Some(card)) => {
                    let dup = matches!(&last, Some((u, t)) if *u == card.uid && t.elapsed() < opts.debounce);
                    last = Some((card.uid.clone(), Instant::now()));
                    if dup {
                        continue;
                    }
                    let fb = on_card(&card);
                    if opts.feedback.load(Ordering::Relaxed) {
                        reader.feedback(fb);
                    }
                }
                Err(e) => {
                    let msg = format!("Lecteur déconnecté : {e}");
                    tracing::warn!("{msg}");
                    on_status(ReaderStatus { connected: false, backend: "none".into(), message: msg });
                    break;
                }
            }
        }
        drop(reader);
        std::thread::sleep(opts.retry.min(Duration::from_millis(500)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn uid_parse_format() {
        assert_eq!(parse_uid("04 24 a5"), Some(vec![0x04, 0x24, 0xA5]));
        assert_eq!(parse_uid("04:24:A5:82:E6:19:90").map(|u| format_uid(&u)).as_deref(), Some("04:24:A5:82:E6:19:90"));
        assert_eq!(parse_uid("0x12"), None);
        assert_eq!(parse_uid("123"), None);
        assert!(CardInfo::new(vec![0x08, 1, 2, 3]).is_random_uid());
        assert!(!CardInfo::new(vec![0x08, 1, 2, 3, 4, 5, 6]).is_random_uid());
    }

    #[test]
    fn loop_debounces_and_stops() {
        let (reader, handle) = MockReader::new();
        let opts = LoopOptions { poll: Duration::from_millis(20), ..Default::default() };
        let stop = opts.stop.clone();
        let seen = Arc::new(Mutex::new(vec![]));
        let seen2 = seen.clone();
        let mut slot = Some(Box::new(reader) as Box<dyn CardReader>);
        let t = std::thread::spawn(move || {
            run_loop(
                move || slot.take().ok_or(NfcError::NoReader("mock déjà pris".into())),
                opts,
                |_| {},
                move |c| {
                    seen2.lock().unwrap().push(c.uid_hex());
                    Feedback::Ok
                },
            )
        });
        handle.inject(vec![1, 2]);
        handle.inject(vec![1, 2]); // rebond ignoré
        handle.inject(vec![3, 4]);
        std::thread::sleep(Duration::from_millis(200));
        stop.store(true, Ordering::Relaxed);
        t.join().unwrap();
        assert_eq!(*seen.lock().unwrap(), vec!["01:02", "03:04"]);
    }
}
