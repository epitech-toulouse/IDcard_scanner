//! Configuration de la borne (`/etc/badgeuse-kiosk.toml`).

use badgeuse_core::nfc::{Beep, ReaderKind};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Adresse de l'appli Badgeuse, ex. `http://pc-prof.local:8737` (HTTP, réseau local).
    pub server_url: String,
    /// Jeton affiché dans Réglages → Bornes réseau de l'appli.
    pub token: String,
    /// Nom de cette borne (lettres, chiffres, `-` et `_`) : sert à identifier ses badges.
    pub device: String,
    /// auto | pcsc | mock
    pub reader: String,
    /// Adresse du petit serveur d'affichage. Toujours locale : la page n'est pas authentifiée.
    pub listen: String,
    /// File d'attente des badges non transmis.
    pub queue_path: PathBuf,
    /// LED et bip du lecteur.
    pub feedback: bool,
    /// normal | discreet | silent
    pub beep: String,
    /// Clé publique (hexadécimal) affichée dans Réglages → Bornes réseau → Mise à jour. Vide : la borne
    /// ne se met jamais à jour toute seule.
    pub update_public_key: String,
    /// Dossier où la borne dépose le paquet vérifié, en attente d'installation par le service root.
    pub update_dir: PathBuf,
    /// Durée d'affichage du résultat d'un badge, en secondes (1 à 30).
    pub show_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server_url: String::new(),
            token: String::new(),
            device: "borne1".into(),
            reader: "auto".into(),
            listen: "127.0.0.1:8080".into(),
            queue_path: "/var/lib/badgeuse-kiosk/queue.db".into(),
            feedback: true,
            beep: "normal".into(),
            update_public_key: String::new(),
            update_dir: "/var/lib/badgeuse-kiosk/update".into(),
            show_secs: 2,
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        let mut cfg: Config = toml::from_str(text).map_err(|e| format!("configuration illisible : {e}"))?;
        cfg.server_url = cfg.server_url.trim().trim_end_matches('/').to_string();
        cfg.token = cfg.token.trim().to_string();
        cfg.device = cfg.device.trim().to_string();
        cfg.update_public_key = cfg.update_public_key.trim().to_ascii_lowercase();
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{} : {e}", path.display()))?;
        Config::parse(&text)
    }

    fn validate(&self) -> Result<(), String> {
        if !self.server_url.starts_with("http://") {
            return Err("server_url doit commencer par http:// (réseau local, sans TLS)".into());
        }
        if self.token.is_empty() {
            return Err("token est obligatoire (voir Réglages → Bornes réseau)".into());
        }
        if self.device.is_empty() || !self.device.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err("device : lettres, chiffres, « - » et « _ » uniquement".into());
        }
        if matches!(self.reader_kind()?, ReaderKind::Usb) {
            return Err("reader = usb n'est pas pris en charge sur la borne (utilisez auto, pcsc ou mock)".into());
        }
        if !self.update_public_key.is_empty()
            && !(self.update_public_key.len() == 64 && self.update_public_key.chars().all(|c| c.is_ascii_hexdigit()))
        {
            return Err("update_public_key : 64 caractères hexadécimaux attendus (copier depuis l'appli)".into());
        }
        if !(1..=30).contains(&self.show_secs) {
            return Err("show_secs doit être compris entre 1 et 30".into());
        }
        let addr: SocketAddr = self.listen.parse().map_err(|_| format!("listen invalide : {}", self.listen))?;
        if !addr.ip().is_loopback() {
            return Err("listen doit être une adresse locale (127.0.0.1) : l'affichage n'est pas authentifié".into());
        }
        Ok(())
    }

    pub fn reader_kind(&self) -> Result<ReaderKind, String> {
        self.reader.parse()
    }

    pub fn beep_level(&self) -> Beep {
        Beep::parse(&self.beep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"
        server_url = "http://192.168.1.20:8737/"
        token = " abc "
    "#;

    #[test]
    fn minimal_config_gets_defaults() {
        let c = Config::parse(OK).unwrap();
        assert_eq!(c.server_url, "http://192.168.1.20:8737");
        assert_eq!((c.token.as_str(), c.device.as_str(), c.reader.as_str()), ("abc", "borne1", "auto"));
        assert_eq!(c.listen, "127.0.0.1:8080");
        assert!(c.feedback);
        assert_eq!(c.show_secs, 2);
    }

    #[test]
    fn rejects_unsafe_or_incomplete_config() {
        assert!(Config::parse("").is_err()); // ni adresse ni jeton
        assert!(Config::parse(r#"server_url = "https://x""#).is_err()); // pas de TLS embarqué
        assert!(Config::parse("server_url = \"http://x\"\ntoken = \"\"").is_err());
        let with = |extra: &str| Config::parse(&format!("{OK}\n{extra}"));
        assert!(with("device = \"salle 1\"").is_err());
        assert!(with("device = \"salle-1\"").is_ok());
        assert!(with("reader = \"usb\"").is_err());
        assert!(with("reader = \"mock\"").is_ok());
        assert!(with("listen = \"0.0.0.0:8080\"").is_err()); // l'affichage reste local
        assert!(with("listen = \"nimporte\"").is_err());
        assert!(with("show_secs = 0").is_err());
        assert!(with("show_secs = 31").is_err());
        assert!(with("show_secs = 4").is_ok());
        assert!(with("inconnu = 1").is_err()); // faute de frappe signalée
        assert!(with("update_public_key = \"abc\"").is_err());
        assert!(with(&format!("update_public_key = \"{}\"", "AB".repeat(32))).is_ok());
        assert!(Config::parse(OK).unwrap().update_public_key.is_empty()); // désactivé par défaut
    }
}
