//! Protocole entre une borne de scan (Raspberry Pi) et l'appli, sur le réseau local.
//! Types échangés en JSON + règles pures (horloge, jeton, réponse minimale pour l'écran de la borne).
//! L'enregistrement en base est dans `attendance::record_remote_scan`.

use crate::attendance::ScanOutcome;
use crate::model::Status;
use crate::nfc::Feedback;
use chrono::{Duration, NaiveDateTime};
use serde::{Deserialize, Serialize};

/// Durée de conservation des badges reçus (idempotence + audit).
pub const PURGE_DAYS: i64 = 30;

/// Écart d'horloge toléré sans correction (latence réseau, dérive normale).
const CLOCK_TOLERANCE_SECS: i64 = 2;

/// Un badge envoyé par une borne. `scan_id` (`"<borne>-<n°>"`) rend le renvoi idempotent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteScan {
    pub scan_id: String,
    pub device: String,
    pub uid: String,
    /// Heure du badge selon l'horloge de la borne.
    pub scanned_at: NaiveDateTime,
    /// Heure de la borne à l'envoi : sert à mesurer l'écart avec l'horloge de l'appli.
    pub device_now: NaiveDateTime,
}

/// Au-delà de cet âge (badge → envoi), un badge vient de la file d'attente de la borne, pas d'un passage en direct.
const REPLAY_AFTER_SECS: i64 = 10;

impl RemoteScan {
    /// Badge rejoué depuis la file d'attente : en mode « Test cartes », l'appli le refuse (la borne
    /// le garde) au lieu de le consommer comme un simple diagnostic.
    pub fn is_replay(&self) -> bool {
        self.device_now - self.scanned_at > Duration::seconds(REPLAY_AFTER_SECS)
    }
}

/// Signal de vie d'une borne.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteHeartbeat {
    pub device: String,
    /// Badges en attente d'envoi dans la file de la borne.
    pub pending: u32,
    /// Version installée sur la borne (absente des anciennes bornes).
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyKind {
    Recorded,
    AlreadyScanned,
    NotExpected,
    UnknownCard,
    Identified,
    SessionNotOpen,
    RandomUid,
}

/// Ce que la borne affiche : le strict nécessaire (prénom + initiale, jamais le numéro ni l'e-mail).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteReply {
    pub kind: ReplyKind,
    pub display_name: Option<String>,
    pub status: Option<Status>,
    pub message: String,
    /// LED / bip à produire sur le lecteur de la borne.
    pub feedback: Feedback,
    /// Intitulé du créneau sur lequel le badge a été traité.
    #[serde(default)]
    pub session_title: Option<String>,
}

/// Réponse au signal de vie : de quoi afficher l'état du créneau en attendant un badge.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HeartbeatReply {
    /// Intitulé du créneau ouvert qui reçoit les badges, s'il y en a un.
    pub session_title: Option<String>,
    /// Mise à jour publiée dans l'appli, s'il y en a une (la borne décide si elle est plus récente).
    #[serde(default)]
    pub update: Option<crate::update::UpdateOffer>,
}

/// Résultat du traitement d'un badge distant. `outcome` vaut `None` pour un rejeu déjà traité
/// (la réponse d'origine est renvoyée, rien n'est ré-émis vers l'interface).
#[derive(Debug, Clone)]
pub struct RemoteRecorded {
    pub reply: RemoteReply,
    pub outcome: Option<ScanOutcome>,
}

fn short_name(first: &str, last: &str) -> String {
    match last.trim().chars().next() {
        Some(i) => format!("{} {}.", first.trim(), i.to_uppercase()),
        None => first.trim().to_string(),
    }
}

impl RemoteReply {
    pub fn from_outcome(o: &ScanOutcome) -> Self {
        let feedback = o.feedback();
        let reply = |kind, name: Option<String>, status, message: &str| RemoteReply {
            kind,
            display_name: name,
            status,
            message: message.into(),
            feedback,
            session_title: None,
        };
        match o {
            ScanOutcome::Recorded { student, status, .. } => reply(
                ReplyKind::Recorded,
                Some(short_name(&student.first_name, &student.last_name)),
                Some(*status),
                if *status == Status::Late { "Enregistré : en retard" } else { "Présence enregistrée" },
            ),
            ScanOutcome::AlreadyScanned { student, status, .. } => reply(
                ReplyKind::AlreadyScanned,
                Some(short_name(&student.first_name, &student.last_name)),
                Some(*status),
                "Déjà enregistré",
            ),
            ScanOutcome::NotExpected { student, .. } => reply(
                ReplyKind::NotExpected,
                Some(short_name(&student.first_name, &student.last_name)),
                None,
                "Vous n'êtes pas attendu sur ce créneau",
            ),
            ScanOutcome::UnknownCard { .. } => {
                reply(ReplyKind::UnknownCard, None, None, "Carte inconnue, voir l'enseignant")
            }
            ScanOutcome::Identified { student: Some(s), .. } => reply(
                ReplyKind::Identified,
                Some(short_name(&s.first_name, &s.last_name)),
                None,
                "Carte reconnue, aucun créneau ouvert",
            ),
            ScanOutcome::Identified { student: None, .. } => {
                reply(ReplyKind::UnknownCard, None, None, "Carte inconnue, voir l'enseignant")
            }
            ScanOutcome::SessionNotOpen { .. } => reply(ReplyKind::SessionNotOpen, None, None, "Aucun créneau ouvert"),
            ScanOutcome::RandomUid { .. } => {
                reply(ReplyKind::RandomUid, None, None, "Carte non utilisable (identifiant aléatoire)")
            }
        }
    }
}

/// Ramène l'heure d'un badge sur l'horloge de l'appli. Le Pi 4 n'a pas d'horloge à pile : démarré
/// hors ligne, son heure peut être fausse. L'écart est mesuré à l'envoi (`server_now − device_now`)
/// et appliqué à l'heure du badge ; en dessous de 2 s on ne touche à rien.
pub fn correct_clock(scanned_at: NaiveDateTime, device_now: NaiveDateTime, server_now: NaiveDateTime) -> NaiveDateTime {
    let skew = server_now - device_now;
    if skew.num_seconds().abs() <= CLOCK_TOLERANCE_SECS {
        scanned_at
    } else {
        scanned_at + skew
    }
}

/// Comparaison de jetons en temps constant (pas de fuite par la durée de la comparaison).
pub fn token_matches(expected: &str, given: &str) -> bool {
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0)) as usize;
    }
    diff == 0
}

/// Nouveau jeton d'authentification : 128 bits aléatoires, en hexadécimal.
pub fn generate_token() -> crate::Result<String> {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).map_err(|e| crate::Error::Invalid(format!("Génération du jeton impossible : {e}")))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// Instant avant lequel les badges reçus sont purgés.
pub fn purge_before(now: NaiveDateTime) -> NaiveDateTime {
    now - Duration::days(PURGE_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Student;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn student() -> Student {
        Student {
            id: 1,
            student_number: "42".into(),
            last_name: "dupont".into(),
            first_name: "Alice".into(),
            email: Some("a@x.fr".into()),
            cohort_id: None,
            active: true,
        }
    }

    #[test]
    fn clock_corrected_only_beyond_tolerance() {
        let badge = dt("2026-11-17 08:00:00");
        // Borne en retard de 2 h : on rattrape l'écart.
        let (dev, srv) = (dt("2026-11-17 06:30:10"), dt("2026-11-17 08:30:10"));
        assert_eq!(correct_clock(badge, dev, srv), dt("2026-11-17 10:00:00"));
        // Borne en avance : écart négatif.
        assert_eq!(correct_clock(badge, srv, dev), dt("2026-11-17 06:00:00"));
        // Latence normale : inchangé.
        assert_eq!(correct_clock(badge, dt("2026-11-17 08:30:10"), dt("2026-11-17 08:30:11")), badge);
    }

    #[test]
    fn replay_detection() {
        let mk = |at: &str, now: &str| RemoteScan {
            scan_id: "b-1".into(),
            device: "b".into(),
            uid: "04:A1".into(),
            scanned_at: dt(at),
            device_now: dt(now),
        };
        assert!(!mk("2026-11-17 08:00:00", "2026-11-17 08:00:03").is_replay());
        assert!(!mk("2026-11-17 08:00:00", "2026-11-17 08:00:10").is_replay());
        assert!(mk("2026-11-17 08:00:00", "2026-11-17 08:00:11").is_replay());
    }

    #[test]
    fn token_comparison() {
        assert!(token_matches("abc123", "abc123"));
        assert!(!token_matches("abc123", "abc124"));
        assert!(!token_matches("abc123", "abc12"));
        assert!(!token_matches("abc123", ""));
        assert!(token_matches("", ""));
        let t = generate_token().unwrap();
        assert_eq!(t.len(), 32);
        assert_ne!(t, generate_token().unwrap());
    }

    #[test]
    fn reply_is_minimal() {
        let o = ScanOutcome::Recorded {
            uid: "04:A1".into(),
            student: student(),
            status: Status::Late,
            scanned_at: dt("2026-11-17 08:15:00"),
        };
        let r = RemoteReply::from_outcome(&o);
        assert_eq!(r.display_name.as_deref(), Some("Alice D."));
        assert_eq!((r.kind, r.status, r.feedback), (ReplyKind::Recorded, Some(Status::Late), Feedback::Warn));
        // Ni numéro étudiant, ni e-mail, ni nom complet dans ce qui part vers la borne.
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("42") && !json.contains("a@x.fr") && !json.to_lowercase().contains("dupont"));

        let unknown = RemoteReply::from_outcome(&ScanOutcome::UnknownCard { uid: "04:FF".into() });
        assert_eq!((unknown.kind, unknown.feedback), (ReplyKind::UnknownCard, Feedback::Error));
        let none = RemoteReply::from_outcome(&ScanOutcome::Identified { uid: "04:FF".into(), student: None });
        assert_eq!(none.kind, ReplyKind::UnknownCard);
    }
}
