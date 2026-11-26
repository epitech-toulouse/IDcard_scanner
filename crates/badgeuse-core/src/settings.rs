//! Réglages clé/valeur.

use crate::Result;
use crate::nfc::Beep;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Seuil de retard proposé à la création d'un créneau (minutes).
    pub default_late_min: i64,
    /// Durée par défaut d'un créneau (minutes).
    pub default_duration_min: i64,
    /// auto | pcsc | usb | mock — pris en compte au prochain démarrage.
    pub reader: String,
    /// Bip / LED du lecteur.
    pub reader_feedback: bool,
    /// Niveau du bip quand le retour lecteur est actif : normal | discreet | silent.
    pub reader_beep: Beep,
    /// Ouvre automatiquement les créneaux prévus à l'approche de leur début.
    pub auto_open: bool,
    /// Avance de l'ouverture automatique (minutes avant le début).
    pub auto_open_lead_min: i64,
    /// Clôt automatiquement les créneaux ouverts à leur fin (les non-badgés passent absents).
    pub auto_close: bool,
    /// Accepte les badges des bornes réseau (désactivé par défaut) — pris en compte au prochain démarrage.
    pub remote_enabled: bool,
    /// Port d'écoute des bornes (1024–65535).
    pub remote_port: i64,
    /// Jeton que les bornes présentent ; généré à la première activation.
    pub remote_token: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            default_late_min: 10, default_duration_min: 120, reader: "auto".into(), reader_feedback: true,
            reader_beep: Beep::Normal,
            auto_open: true,
            auto_open_lead_min: 10,
            auto_close: false,
            remote_enabled: false,
            remote_port: 8737,
            remote_token: String::new(),
        }
    }
}

pub(crate) fn get_raw(c: &Connection, key: &str) -> Result<Option<String>> {
    Ok(c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub(crate) fn set_raw(c: &Connection, key: &str, value: &str) -> Result<()> {
    c.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn load(c: &Connection) -> Result<Settings> {
    let d = Settings::default();
    let num = |k: &str, def: i64| -> Result<i64> { Ok(get_raw(c, k)?.and_then(|v| v.parse().ok()).unwrap_or(def)) };
    let flag = |k: &str, def: bool| -> Result<bool> { Ok(get_raw(c, k)?.map(|v| v == "1").unwrap_or(def)) };
    Ok(Settings {
        default_late_min: num("default_late_min", d.default_late_min)?,
        default_duration_min: num("default_duration_min", d.default_duration_min)?,
        reader: get_raw(c, "reader")?.unwrap_or(d.reader),
        reader_feedback: get_raw(c, "reader_feedback")?.map(|v| v == "1").unwrap_or(d.reader_feedback),
        reader_beep: get_raw(c, "reader_beep")?.map(|v| Beep::parse(&v)).unwrap_or(d.reader_beep),
        auto_open: flag("auto_open", d.auto_open)?,
        auto_open_lead_min: num("auto_open_lead_min", d.auto_open_lead_min)?,
        auto_close: flag("auto_close", d.auto_close)?,
        remote_enabled: flag("remote_enabled", d.remote_enabled)?,
        remote_port: num("remote_port", d.remote_port)?,
        remote_token: get_raw(c, "remote_token")?.unwrap_or(d.remote_token),
    })
}

pub fn save(c: &Connection, s: &Settings) -> Result<()> {
    set_raw(c, "default_late_min", &s.default_late_min.max(0).to_string())?;
    set_raw(c, "default_duration_min", &s.default_duration_min.max(5).to_string())?;
    set_raw(c, "reader", &s.reader)?;
    set_raw(c, "reader_feedback", if s.reader_feedback { "1" } else { "0" })?;
    set_raw(c, "reader_beep", s.reader_beep.as_str())?;
    set_raw(c, "auto_open", if s.auto_open { "1" } else { "0" })?;
    set_raw(c, "auto_open_lead_min", &s.auto_open_lead_min.clamp(0, 120).to_string())?;
    set_raw(c, "auto_close", if s.auto_close { "1" } else { "0" })?;
    set_raw(c, "remote_enabled", if s.remote_enabled { "1" } else { "0" })?;
    set_raw(c, "remote_port", &s.remote_port.clamp(1024, 65535).to_string())?;
    // Une borne n'est jamais acceptée sans jeton : on en génère un à l'activation.
    let token = if s.remote_enabled && s.remote_token.trim().is_empty() {
        crate::remote::generate_token()?
    } else {
        s.remote_token.trim().to_string()
    };
    set_raw(c, "remote_token", &token)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip() {
        let c = crate::db::open_in_memory().unwrap();
        let mut s = super::load(&c).unwrap();
        assert_eq!(s.default_late_min, 10);
        s.reader = "usb".into();
        assert_eq!(s.reader_beep, crate::nfc::Beep::Normal);
        s.reader_feedback = false;
        s.reader_beep = crate::nfc::Beep::Discreet;
        assert!(s.auto_open && !s.auto_close);
        s.auto_close = true;
        super::save(&c, &s).unwrap();
        let s2 = super::load(&c).unwrap();
        assert_eq!((s2.reader.as_str(), s2.reader_feedback, s2.reader_beep), ("usb", false, crate::nfc::Beep::Discreet));
        assert!(s2.auto_close);
    }

    #[test]
    fn remote_token_generated_on_enable() {
        let c = crate::db::open_in_memory().unwrap();
        let mut s = super::load(&c).unwrap();
        assert!(!s.remote_enabled && s.remote_token.is_empty() && s.remote_port == 8737);
        super::save(&c, &s).unwrap();
        assert!(super::load(&c).unwrap().remote_token.is_empty());
        s.remote_enabled = true;
        s.remote_port = 80; // hors plage : ramené à 1024
        super::save(&c, &s).unwrap();
        let s2 = super::load(&c).unwrap();
        assert_eq!((s2.remote_enabled, s2.remote_port, s2.remote_token.len()), (true, 1024, 32));
        // Le jeton existant est conservé.
        super::save(&c, &s2).unwrap();
        assert_eq!(super::load(&c).unwrap().remote_token, s2.remote_token);
    }
}
