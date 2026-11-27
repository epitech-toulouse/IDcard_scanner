//! Cœur de la borne : un badge est écrit dans la file locale, envoyé à l'appli, puis retiré de la file
//! dès que l'appli l'a traité. Si l'appli est injoignable, il reste en file et part plus tard, avec son
//! heure d'origine (`flush`, appelé toutes les 10 s).

use crate::client::{SendError, Transport};
use crate::display::{Display, Network, Shown};
use crate::queue::{Pending, Queue};
use badgeuse_core::nfc::{CardInfo, Feedback};
use crate::updater::{self, UpdateConfig};
use badgeuse_core::remote::{RemoteReply, RemoteScan};
use badgeuse_core::update::{self, UpdateOffer};
use chrono::NaiveDateTime;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// La borne ne se met à jour que si personne n'a badgé depuis ce délai (jamais en plein appel).
const UPDATE_IDLE_FOR: Duration = Duration::from_secs(60);
/// Après un échec de téléchargement ou de vérification, on ne réessaie pas la même version avant ce délai.
const UPDATE_RETRY_AFTER: Duration = Duration::from_secs(600);

/// Badges renvoyés par lot lors du vidage de la file.
const BATCH: u32 = 50;
/// Garde-fou : nombre maximal de lots par vidage (évite toute boucle si la file ne diminue pas).
const MAX_BATCHES: u32 = 20;

pub struct Kiosk {
    device: String,
    queue: Mutex<Queue>,
    transport: Arc<dyn Transport>,
    pub display: Display,
    clock: Box<dyn Fn() -> NaiveDateTime + Send + Sync>,
    updates: Option<UpdateConfig>,
    last_card: Mutex<Option<Instant>>,
    /// Dernière version dont la préparation a échoué, et quand.
    update_failed: Mutex<Option<(String, Instant)>>,
}

/// Sort d'un badge après une tentative d'envoi.
enum Delivery {
    /// L'appli l'a traité : on a sa réponse, le badge a quitté la file.
    Replied(RemoteReply),
    /// Appli injoignable (ou jeton refusé) : le badge reste en file.
    Queued,
    /// L'appli l'a refusé pour de bon : abandonné.
    Rejected,
}

impl Kiosk {
    pub fn new(
        device: String,
        queue: Queue,
        transport: Arc<dyn Transport>,
        clock: Box<dyn Fn() -> NaiveDateTime + Send + Sync>,
    ) -> Kiosk {
        Kiosk {
            device,
            queue: Mutex::new(queue),
            transport,
            display: Display::new(),
            clock,
            updates: None,
            last_card: Mutex::new(None),
            update_failed: Mutex::new(None),
        }
    }

    /// Active la mise à jour depuis l'appli (clé publique configurée).
    pub fn with_updates(mut self, cfg: UpdateConfig) -> Kiosk {
        self.updates = Some(cfg);
        self
    }

    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn refresh_pending(&self) {
        match self.queue().len() {
            Ok(n) => self.display.set_pending(n),
            Err(e) => tracing::error!("file d'attente illisible : {e}"),
        }
    }

    /// Un carte est posée sur le lecteur : renvoie le retour LED/bip à produire.
    pub fn on_card(&self, card: &CardInfo) -> Feedback {
        let uid = card.uid_hex();
        tracing::info!("carte {uid}");
        *self.last_card.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        if card.is_random_uid() {
            self.display.show(Shown::random_uid());
            return Feedback::Error;
        }
        // Écrit d'abord : une coupure de courant juste après le badge ne le perd pas.
        let pending = match self.queue().push(&uid, (self.clock)()) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("badge {uid} non enregistré : {e}");
                self.display.show(Shown::local_error("Badge non enregistré, réessayez"));
                return Feedback::Error;
            }
        };
        let fb = match self.deliver(&pending) {
            Delivery::Replied(reply) => {
                self.display.show(Shown::from_reply(&reply));
                reply.feedback
            }
            Delivery::Queued => {
                self.display.show(Shown::queued());
                Feedback::Warn
            }
            Delivery::Rejected => {
                self.display.show(Shown::local_error("Badge refusé"));
                Feedback::Error
            }
        };
        self.refresh_pending();
        fb
    }

    fn deliver(&self, p: &Pending) -> Delivery {
        let scan = RemoteScan {
            scan_id: self.queue().scan_id(&self.device, p.seq),
            device: self.device.clone(),
            uid: p.uid.clone(),
            scanned_at: p.scanned_at,
            device_now: (self.clock)(),
        };
        let sent = self.transport.send_scan(&scan);
        match sent {
            Ok(reply) => {
                self.forget(p.seq);
                self.display.set_network(Network::Ok);
                if reply.session_title.is_some() {
                    self.display.set_session(reply.session_title.clone());
                    // Ancienne appli : pas d'avancement, on garde celui du dernier signal de vie.
                    if reply.progress.is_some() {
                        self.display.set_progress(reply.progress);
                    }
                }
                Delivery::Replied(reply)
            }
            Err(SendError::Retry(why)) => {
                tracing::warn!("badge {} gardé en file : {why}", p.uid);
                self.display.set_network(Network::Offline);
                Delivery::Queued
            }
            Err(SendError::Unauthorized) => {
                tracing::error!("jeton refusé par l'appli : badge {} gardé en file", p.uid);
                self.display.set_network(Network::Refused);
                Delivery::Queued
            }
            Err(SendError::Drop(why)) => {
                tracing::error!("badge {} abandonné : {why}", p.uid);
                self.forget(p.seq);
                Delivery::Rejected
            }
        }
    }

    fn forget(&self, seq: i64) {
        if let Err(e) = self.queue().remove(seq) {
            tracing::error!("badge {seq} non retiré de la file : {e}");
        }
    }

    /// Télécharge et dépose la mise à jour proposée par l'appli si elle est signée, plus récente et si la
    /// borne est au repos. L'installation elle-même est faite par le service root (voir `updater`).
    fn prepare_update(&self, offer: &UpdateOffer) {
        let Some(cfg) = &self.updates else { return };
        match updater::wanted(&cfg.public_key, offer, updater::CURRENT) {
            Ok(true) => {}
            Ok(false) => return,
            Err(e) => return self.update_failed(offer, &format!("offre refusée : {e}")),
        }
        if updater::is_staged(&cfg.dir) {
            return;
        }
        if let Some((v, at)) = &*self.update_failed.lock().unwrap_or_else(|e| e.into_inner())
            && *v == offer.version
            && at.elapsed() < UPDATE_RETRY_AFTER
        {
            return;
        }
        let idle = self.last_card.lock().unwrap_or_else(|e| e.into_inner()).is_none_or(|t| t.elapsed() >= UPDATE_IDLE_FOR);
        if !idle || self.queue().len().unwrap_or(1) > 0 {
            return;
        }
        tracing::info!("mise à jour {} → {} : téléchargement", updater::CURRENT, offer.version);
        let staged = self
            .transport
            .fetch_package(offer.size)
            .map_err(|e| format!("téléchargement : {e:?}"))
            .and_then(|data| update::verify_package(offer, &data).map(|()| data).map_err(|e| e.to_string()))
            .and_then(|data| updater::stage(&cfg.dir, offer, &data).map_err(|e| format!("{} : {e}", cfg.dir.display())));
        match staged {
            Ok(()) => tracing::info!("mise à jour {} prête : installation par le service root", offer.version),
            Err(e) => self.update_failed(offer, &e),
        }
    }

    fn update_failed(&self, offer: &UpdateOffer, why: &str) {
        tracing::warn!("mise à jour {} non préparée : {why}", offer.version);
        *self.update_failed.lock().unwrap_or_else(|e| e.into_inner()) = Some((offer.version.clone(), Instant::now()));
    }

    /// Signal de vie, puis renvoi dans l'ordre des badges en attente. S'arrête au premier échec
    /// réseau : les suivants restent en file et repartent au prochain passage.
    pub fn flush(&self) {
        self.refresh_pending();
        let pending = self.queue().len().unwrap_or(0);
        match self.transport.heartbeat(pending) {
            Ok(hb) => {
                self.display.set_network(Network::Ok);
                self.display.set_session(hb.session_title);
                self.display.set_progress(hb.progress);
                if let Some(offer) = hb.update {
                    self.prepare_update(&offer);
                }
            }
            Err(SendError::Retry(why)) => {
                tracing::debug!("appli injoignable : {why}");
                self.display.set_network(Network::Offline);
                return;
            }
            Err(SendError::Unauthorized) => {
                self.display.set_network(Network::Refused);
                return;
            }
            Err(SendError::Drop(why)) => tracing::warn!("signal de vie refusé : {why}"),
        }
        for _ in 0..MAX_BATCHES {
            let batch = match self.queue().oldest(BATCH) {
                Ok(b) if !b.is_empty() => b,
                Ok(_) => break,
                Err(e) => {
                    tracing::error!("file d'attente illisible : {e}");
                    break;
                }
            };
            for p in &batch {
                if let Delivery::Queued = self.deliver(p) {
                    self.refresh_pending();
                    return;
                }
            }
        }
        self.refresh_pending();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use badgeuse_core::model::Status;
    use badgeuse_core::nfc::Feedback;
    use badgeuse_core::remote::{HeartbeatReply, Progress, ReplyKind};
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    /// Appli simulée : joignable ou non, jeton bon ou non, avec un quota d'envois réussis.
    struct FakeApp {
        online: AtomicBool,
        token_ok: AtomicBool,
        budget: AtomicI32,
        received: Mutex<Vec<RemoteScan>>,
        offer: Mutex<Option<UpdateOffer>>,
        package: Mutex<Option<Vec<u8>>>,
    }

    impl Transport for FakeApp {
        fn send_scan(&self, scan: &RemoteScan) -> Result<RemoteReply, SendError> {
            if !self.online.load(Ordering::SeqCst) || self.budget.fetch_sub(1, Ordering::SeqCst) <= 0 {
                return Err(SendError::Retry("réseau coupé".into()));
            }
            if !self.token_ok.load(Ordering::SeqCst) {
                return Err(SendError::Unauthorized);
            }
            if scan.uid == "04:BA:D0" {
                return Err(SendError::Drop("HTTP 422".into()));
            }
            self.received.lock().unwrap().push(scan.clone());
            Ok(RemoteReply {
                kind: ReplyKind::Recorded,
                display_name: Some("Alice D.".into()),
                status: Some(Status::Present),
                message: "Présence enregistrée".into(),
                feedback: Feedback::Ok,
                session_title: Some("Réseaux".into()),
                progress: Some(Progress { here: 11, expected: 24 }),
            })
        }

        fn fetch_package(&self, _max: u64) -> Result<Vec<u8>, SendError> {
            self.package.lock().unwrap().clone().ok_or(SendError::Retry("pas de paquet".into()))
        }

        fn heartbeat(&self, _pending: u32) -> Result<HeartbeatReply, SendError> {
            if !self.online.load(Ordering::SeqCst) {
                return Err(SendError::Retry("réseau coupé".into()));
            }
            if !self.token_ok.load(Ordering::SeqCst) {
                return Err(SendError::Unauthorized);
            }
            Ok(HeartbeatReply {
                session_title: Some("Réseaux".into()),
                update: self.offer.lock().unwrap().clone(),
                progress: Some(Progress { here: 10, expected: 24 }),
            })
        }
    }

    struct Rig {
        kiosk: Kiosk,
        app: Arc<FakeApp>,
        now: Arc<Mutex<NaiveDateTime>>,
    }

    fn rig(online: bool) -> Rig {
        let app = Arc::new(FakeApp {
            online: AtomicBool::new(online),
            token_ok: AtomicBool::new(true),
            budget: AtomicI32::new(i32::MAX),
            received: Mutex::new(vec![]),
            offer: Mutex::new(None),
            package: Mutex::new(None),
        });
        let now = Arc::new(Mutex::new(dt("2026-11-17 08:00:00")));
        let clock = now.clone();
        let kiosk = Kiosk::new("borne1".into(), Queue::open_in_memory(), app.clone(), Box::new(move || *clock.lock().unwrap()));
        Rig { kiosk, app, now }
    }

    fn card(last: u8) -> CardInfo {
        CardInfo::new(vec![0x04, 0xA1, last])
    }

    fn state(k: &Kiosk) -> serde_json::Value {
        serde_json::from_str(&k.display.state_json()).unwrap()
    }

    fn pending(k: &Kiosk) -> u32 {
        k.queue().len().unwrap()
    }

    #[test]
    fn online_badge_is_sent_and_leaves_the_queue() {
        let r = rig(true);
        assert_eq!(r.kiosk.on_card(&card(1)), Feedback::Ok);
        assert_eq!(pending(&r.kiosk), 0);
        let got = r.app.received.lock().unwrap().clone();
        assert_eq!((got.len(), got[0].uid.as_str(), got[0].device.as_str()), (1, "04:A1:01", "borne1"));
        assert!(got[0].scan_id.starts_with("borne1-"));
        let s = state(&r.kiosk);
        assert_eq!((s["network"].as_str(), s["last"]["name"].as_str(), s["session"].as_str()), (Some("ok"), Some("Alice D."), Some("Réseaux")));
        assert_eq!(s["progress"]["here"].as_u64(), Some(11));
        // Le signal de vie remet l'avancement à jour.
        r.kiosk.flush();
        assert_eq!(state(&r.kiosk)["progress"]["here"].as_u64(), Some(10));
    }

    #[test]
    fn offline_badges_wait_then_replay_in_order_with_original_time() {
        let r = rig(false);
        for (i, t) in ["08:00:00", "08:00:20", "08:00:40"].iter().enumerate() {
            *r.now.lock().unwrap() = dt(&format!("2026-11-17 {t}"));
            assert_eq!(r.kiosk.on_card(&card(i as u8 + 1)), Feedback::Warn);
        }
        assert_eq!(pending(&r.kiosk), 3);
        let s = state(&r.kiosk);
        assert_eq!((s["network"].as_str(), s["pending"].as_u64(), s["last"]["kind"].as_str()), (Some("offline"), Some(3), Some("queued")));
        // Pas de nom affiché hors ligne, et jamais l'UID.
        assert!(s["last"]["name"].is_null() && !s.to_string().contains("04:A1"));

        // Tant que le réseau est coupé, le vidage ne perd rien.
        r.kiosk.flush();
        assert_eq!(pending(&r.kiosk), 3);

        // Retour du réseau 5 minutes plus tard : tout part, dans l'ordre, avec l'heure d'origine.
        r.app.online.store(true, Ordering::SeqCst);
        *r.now.lock().unwrap() = dt("2026-11-17 08:05:00");
        r.kiosk.flush();
        assert_eq!(pending(&r.kiosk), 0);
        let got = r.app.received.lock().unwrap().clone();
        let uids: Vec<_> = got.iter().map(|s| s.uid.as_str()).collect();
        assert_eq!(uids, ["04:A1:01", "04:A1:02", "04:A1:03"]);
        assert_eq!(got[0].scanned_at, dt("2026-11-17 08:00:00"));
        assert_eq!(got[2].scanned_at, dt("2026-11-17 08:00:40"));
        assert!(got.iter().all(|s| s.is_replay()), "l'appli doit reconnaître des badges de la file");
        assert_eq!(state(&r.kiosk)["network"].as_str(), Some("ok"));
        // Les identifiants sont distincts (sinon l'appli prendrait deux badges pour un).
        let mut ids: Vec<_> = got.iter().map(|s| s.scan_id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn flush_stops_at_first_failure_and_keeps_the_rest() {
        let r = rig(false);
        for i in 1..=3 {
            r.kiosk.on_card(&card(i));
        }
        r.app.online.store(true, Ordering::SeqCst);
        r.app.budget.store(1, Ordering::SeqCst); // un seul envoi réussit, puis le réseau retombe
        r.kiosk.flush();
        assert_eq!(pending(&r.kiosk), 2);
        let got = r.app.received.lock().unwrap().clone();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].uid, "04:A1:01"); // le plus ancien d'abord
        assert_eq!(state(&r.kiosk)["pending"].as_u64(), Some(2));
    }

    #[test]
    fn refused_token_keeps_badges_and_says_so() {
        let r = rig(true);
        r.app.token_ok.store(false, Ordering::SeqCst);
        assert_eq!(r.kiosk.on_card(&card(1)), Feedback::Warn);
        assert_eq!(pending(&r.kiosk), 1);
        assert_eq!(state(&r.kiosk)["network"].as_str(), Some("refused"));
        // Une fois le jeton corrigé, le badge part.
        r.app.token_ok.store(true, Ordering::SeqCst);
        r.kiosk.flush();
        assert_eq!((pending(&r.kiosk), r.app.received.lock().unwrap().len()), (0, 1));
    }

    #[test]
    fn rejected_badge_is_dropped_not_replayed_forever() {
        let r = rig(true);
        assert_eq!(r.kiosk.on_card(&CardInfo::new(vec![0x04, 0xBA, 0xD0])), Feedback::Error);
        assert_eq!(pending(&r.kiosk), 0);
        assert!(r.app.received.lock().unwrap().is_empty());
    }

    #[test]
    fn random_uid_is_refused_locally() {
        let r = rig(true);
        assert_eq!(r.kiosk.on_card(&CardInfo::new(vec![0x08, 1, 2, 3])), Feedback::Error);
        assert_eq!(pending(&r.kiosk), 0);
        assert!(r.app.received.lock().unwrap().is_empty());
        assert_eq!(state(&r.kiosk)["last"]["kind"].as_str(), Some("random_uid"));
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("badgeuse-kiosk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Borne avec mise à jour activée ; l'appli propose `version` signée par la bonne clé.
    fn update_rig(name: &str, version: &str, data: &[u8]) -> (Rig, std::path::PathBuf) {
        let (secret, public) = update::generate_keypair().unwrap();
        let dir = temp_dir(name);
        let mut r = rig(true);
        r.kiosk = Kiosk::new("borne1".into(), Queue::open_in_memory(), r.app.clone(), Box::new(|| dt("2026-11-17 08:00:00")))
            .with_updates(UpdateConfig { public_key: public, dir: dir.clone() });
        *r.app.offer.lock().unwrap() = Some(update::make_offer(&secret, version, data).unwrap());
        *r.app.package.lock().unwrap() = Some(data.to_vec());
        (r, dir)
    }

    #[test]
    fn newer_signed_update_is_downloaded_and_staged() {
        let (r, dir) = update_rig("stage", "99.0.0", b"paquet");
        r.kiosk.flush();
        assert!(updater::is_staged(&dir));
        assert_eq!(std::fs::read(dir.join(updater::PENDING_DEB)).unwrap(), b"paquet");
    }

    #[test]
    fn update_not_staged_when_old_corrupt_busy_or_disabled() {
        // Version pas plus récente que la borne.
        let (r, dir) = update_rig("old", "0.0.1", b"paquet");
        r.kiosk.flush();
        assert!(!updater::is_staged(&dir));

        // Paquet servi différent de celui qui est signé.
        let (r, dir) = update_rig("corrupt", "99.0.0", b"paquet");
        *r.app.package.lock().unwrap() = Some(b"autre chose".to_vec());
        r.kiosk.flush();
        assert!(!updater::is_staged(&dir));

        // Quelqu'un vient de badger : on attend.
        let (r, dir) = update_rig("busy", "99.0.0", b"paquet");
        r.kiosk.on_card(&card(1));
        r.kiosk.flush();
        assert!(!updater::is_staged(&dir));

        // Badge encore en file : on attend aussi.
        let (r, dir) = update_rig("queued", "99.0.0", b"paquet");
        r.app.online.store(false, Ordering::SeqCst);
        *r.kiosk.last_card.lock().unwrap() = None;
        r.kiosk.queue().push("04:A1:01", dt("2026-11-17 08:00:00")).unwrap();
        r.app.online.store(true, Ordering::SeqCst);
        r.app.budget.store(0, Ordering::SeqCst); // l'envoi du badge échoue : il reste en file
        r.kiosk.flush();
        assert!(!updater::is_staged(&dir));

        // Sans clé publique configurée (cas par défaut), jamais de mise à jour.
        let r = rig(true);
        *r.app.offer.lock().unwrap() = Some(update::make_offer(&update::generate_keypair().unwrap().0, "99.0.0", b"x").unwrap());
        *r.app.package.lock().unwrap() = Some(b"x".to_vec());
        r.kiosk.flush();
    }

    #[test]
    fn offer_signed_with_another_key_is_never_downloaded() {
        let (r, dir) = update_rig("forged", "99.0.0", b"paquet");
        let (other_secret, _) = update::generate_keypair().unwrap();
        *r.app.offer.lock().unwrap() = Some(update::make_offer(&other_secret, "99.0.0", b"paquet").unwrap());
        r.kiosk.flush();
        assert!(!updater::is_staged(&dir));
    }
}
