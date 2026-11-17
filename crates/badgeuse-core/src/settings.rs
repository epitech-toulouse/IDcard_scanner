//! Réglages clé/valeur.

use crate::Result;
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings { default_late_min: 10, default_duration_min: 120, reader: "auto".into(), reader_feedback: true }
    }
}

fn get_raw(c: &Connection, key: &str) -> Result<Option<String>> {
    Ok(c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

fn set_raw(c: &Connection, key: &str, value: &str) -> Result<()> {
    c.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn load(c: &Connection) -> Result<Settings> {
    let d = Settings::default();
    let num = |k: &str, def: i64| -> Result<i64> { Ok(get_raw(c, k)?.and_then(|v| v.parse().ok()).unwrap_or(def)) };
    Ok(Settings {
        default_late_min: num("default_late_min", d.default_late_min)?,
        default_duration_min: num("default_duration_min", d.default_duration_min)?,
        reader: get_raw(c, "reader")?.unwrap_or(d.reader),
        reader_feedback: get_raw(c, "reader_feedback")?.map(|v| v == "1").unwrap_or(d.reader_feedback),
    })
}

pub fn save(c: &Connection, s: &Settings) -> Result<()> {
    set_raw(c, "default_late_min", &s.default_late_min.max(0).to_string())?;
    set_raw(c, "default_duration_min", &s.default_duration_min.max(5).to_string())?;
    set_raw(c, "reader", &s.reader)?;
    set_raw(c, "reader_feedback", if s.reader_feedback { "1" } else { "0" })?;
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
        s.reader_feedback = false;
        super::save(&c, &s).unwrap();
        let s2 = super::load(&c).unwrap();
        assert_eq!((s2.reader.as_str(), s2.reader_feedback), ("usb", false));
    }
}
