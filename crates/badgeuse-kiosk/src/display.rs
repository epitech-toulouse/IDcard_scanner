//! État affiché sur l'écran HDMI, partagé entre la lecture NFC, l'envoi et le petit serveur web local.

use badgeuse_core::model::Status;
use badgeuse_core::nfc::Feedback;
use badgeuse_core::remote::{RemoteReply, ReplyKind};
use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Durée d'affichage du résultat d'un badge en grand.
const SHOW_FOR: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    /// Pas encore de réponse de l'appli depuis le démarrage.
    Unknown,
    Ok,
    Offline,
    /// L'appli répond mais refuse le jeton.
    Refused,
}

/// Résultat d'un badge tel que la page l'affiche.
#[derive(Debug, Clone, Serialize)]
pub struct Shown {
    /// `recorded`, `already_scanned`…, ou `queued` (badge gardé, appli injoignable).
    pub kind: String,
    pub name: Option<String>,
    pub status: Option<Status>,
    pub message: String,
    pub feedback: Feedback,
}

impl Shown {
    pub fn from_reply(r: &RemoteReply) -> Shown {
        let kind = serde_json::to_value(r.kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        Shown { kind, name: r.display_name.clone(), status: r.status, message: r.message.clone(), feedback: r.feedback }
    }

    /// Badge gardé en file : l'appli n'a pas pu répondre, donc pas de nom à afficher.
    pub fn queued() -> Shown {
        Shown {
            kind: "queued".into(),
            name: None,
            status: None,
            message: "Badge enregistré, transmission en attente".into(),
            feedback: Feedback::Warn,
        }
    }

    pub fn local_error(message: &str) -> Shown {
        Shown { kind: "error".into(), name: None, status: None, message: message.into(), feedback: Feedback::Error }
    }

    pub fn random_uid() -> Shown {
        let r = RemoteReply {
            kind: ReplyKind::RandomUid,
            display_name: None,
            status: None,
            message: "Carte non utilisable (identifiant aléatoire)".into(),
            feedback: Feedback::Error,
            session_title: None,
        };
        Shown::from_reply(&r)
    }
}

struct Inner {
    network: Network,
    pending: u32,
    session: Option<String>,
    readers: usize,
    /// (numéro d'ordre, instant, résultat) du dernier badge.
    last: Option<(u64, Instant, Shown)>,
    seq: u64,
}

pub struct Display {
    inner: Mutex<Inner>,
}

#[derive(Serialize)]
struct LastView<'a> {
    seq: u64,
    #[serde(flatten)]
    shown: &'a Shown,
}

#[derive(Serialize)]
struct StateView<'a> {
    network: Network,
    pending: u32,
    session: Option<&'a str>,
    readers: usize,
    last: Option<LastView<'a>>,
}

impl Display {
    pub fn new() -> Display {
        Display {
            inner: Mutex::new(Inner { network: Network::Unknown, pending: 0, session: None, readers: 0, last: None, seq: 0 }),
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        f(&mut self.inner.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn show(&self, shown: Shown) {
        self.with(|i| {
            i.seq += 1;
            i.last = Some((i.seq, Instant::now(), shown));
        });
    }

    pub fn set_network(&self, n: Network) {
        self.with(|i| i.network = n);
    }

    pub fn set_pending(&self, n: u32) {
        self.with(|i| i.pending = n);
    }

    pub fn set_session(&self, title: Option<String>) {
        self.with(|i| i.session = title);
    }

    pub fn set_readers(&self, n: usize) {
        self.with(|i| i.readers = n);
    }

    /// JSON lu par la page (`GET /state`). Le dernier badge disparaît après quelques secondes.
    pub fn state_json(&self) -> String {
        self.with(|i| {
            let last = i.last.as_ref().filter(|(_, at, _)| at.elapsed() < SHOW_FOR).map(|(seq, _, s)| LastView { seq: *seq, shown: s });
            let view = StateView { network: i.network, pending: i.pending, session: i.session.as_deref(), readers: i.readers, last };
            serde_json::to_string(&view).expect("état sérialisable")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_shows_last_badge_then_hides_it() {
        let d = Display::new();
        let v: serde_json::Value = serde_json::from_str(&d.state_json()).unwrap();
        assert_eq!((v["network"].as_str(), v["pending"].as_u64(), v["last"].is_null()), (Some("unknown"), Some(0), true));

        d.show(Shown::queued());
        d.set_network(Network::Offline);
        d.set_pending(3);
        d.set_session(Some("Réseaux".into()));
        let v: serde_json::Value = serde_json::from_str(&d.state_json()).unwrap();
        assert_eq!((v["network"].as_str(), v["pending"].as_u64(), v["session"].as_str()), (Some("offline"), Some(3), Some("Réseaux")));
        assert_eq!((v["last"]["kind"].as_str(), v["last"]["feedback"].as_str(), v["last"]["seq"].as_u64()), (Some("queued"), Some("warn"), Some(1)));

        // Chaque badge a son numéro : la page s'en sert pour relancer l'animation.
        d.show(Shown::random_uid());
        let v: serde_json::Value = serde_json::from_str(&d.state_json()).unwrap();
        assert_eq!((v["last"]["seq"].as_u64(), v["last"]["kind"].as_str()), (Some(2), Some("random_uid")));

        // Expiré : plus affiché.
        d.with(|i| i.last.as_mut().unwrap().1 = Instant::now() - SHOW_FOR - Duration::from_secs(1));
        let v: serde_json::Value = serde_json::from_str(&d.state_json()).unwrap();
        assert!(v["last"].is_null());
    }
}
