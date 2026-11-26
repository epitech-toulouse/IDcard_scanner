//! Mise à jour des bornes depuis l'appli : l'enseignant publie un paquet `.deb` de la borne, l'appli le signe
//! (Ed25519) et le propose aux bornes via leur signal de vie. Une borne n'installe un paquet que si la
//! signature correspond à la clé publique inscrite dans SA configuration : le jeton (qui circule en clair
//! sur le réseau local) ne suffit pas à lui faire installer quoi que ce soit.
//! Ce module ne contient que de la logique pure (hash, signature, versions) ; le disque et le réseau
//! sont gérés par l'appli (`src-tauri`) et par la borne (`badgeuse-kiosk`).

use crate::{Error, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Taille maximale d'un paquet de borne (garde-fou côté appli comme côté borne).
pub const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;

/// Mise à jour proposée aux bornes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateOffer {
    /// Version du paquet, ex. `0.2.0` ou `0.2.0-1`.
    pub version: String,
    /// Empreinte SHA-256 du fichier, en hexadécimal.
    pub sha256: String,
    pub size: u64,
    /// Signature Ed25519 (hexadécimal) de `signing_message`.
    pub signature: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    let s = s.trim();
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Ce qui est signé : la version et la taille sont liées à l'empreinte, pour qu'on ne puisse pas
/// rejouer une vieille signature avec un autre numéro de version.
fn signing_message(version: &str, sha256: &str, size: u64) -> Vec<u8> {
    format!("badgeuse-kiosk-update/v1\n{version}\n{sha256}\n{size}").into_bytes()
}

/// Nouvelle paire de clés : `(clé privée, clé publique)`, en hexadécimal (32 octets chacune).
pub fn generate_keypair() -> Result<(String, String)> {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| Error::Invalid(format!("Génération de clés impossible : {e}")))?;
    let key = SigningKey::from_bytes(&seed);
    Ok((hex(&seed), hex(key.verifying_key().as_bytes())))
}

/// Clé publique correspondant à une clé privée.
pub fn public_key_of(secret_hex: &str) -> Result<String> {
    let seed: [u8; 32] = unhex(secret_hex).ok_or_else(|| Error::Invalid("Clé privée de mise à jour illisible".into()))?;
    Ok(hex(SigningKey::from_bytes(&seed).verifying_key().as_bytes()))
}

/// Signe un paquet (`data` = contenu du `.deb`).
pub fn make_offer(secret_hex: &str, version: &str, data: &[u8]) -> Result<UpdateOffer> {
    if !valid_version(version) {
        return Err(Error::Invalid(format!("Numéro de version invalide : {version}")));
    }
    let seed: [u8; 32] = unhex(secret_hex).ok_or_else(|| Error::Invalid("Clé privée de mise à jour illisible".into()))?;
    let (sha256, size) = (sha256_hex(data), data.len() as u64);
    let sig = SigningKey::from_bytes(&seed).sign(&signing_message(version, &sha256, size));
    Ok(UpdateOffer { version: version.into(), sha256, size, signature: hex(&sig.to_bytes()) })
}

/// Vérifie la signature d'une offre avec la clé publique configurée sur la borne.
pub fn verify_offer(public_hex: &str, offer: &UpdateOffer) -> Result<()> {
    let bad = || Error::Invalid("Signature de mise à jour invalide".into());
    let pk: [u8; 32] = unhex(public_hex).ok_or_else(|| Error::Invalid("Clé publique de mise à jour illisible".into()))?;
    let key = VerifyingKey::from_bytes(&pk).map_err(|_| bad())?;
    let sig: [u8; 64] = unhex(&offer.signature).ok_or_else(bad)?;
    if !valid_version(&offer.version) {
        return Err(bad());
    }
    key.verify(&signing_message(&offer.version, &offer.sha256, offer.size), &Signature::from_bytes(&sig)).map_err(|_| bad())
}

/// Vérifie qu'un fichier téléchargé correspond à l'offre (taille + empreinte).
pub fn verify_package(offer: &UpdateOffer, data: &[u8]) -> Result<()> {
    if data.len() as u64 != offer.size || sha256_hex(data) != offer.sha256 {
        return Err(Error::Invalid("Paquet de mise à jour corrompu (taille ou empreinte différente)".into()));
    }
    Ok(())
}

/// Version acceptable : chiffres séparés par des points, éventuellement suivis d'une révision `-N`.
pub fn valid_version(v: &str) -> bool {
    let (main, rev) = v.split_once('-').map_or((v, None), |(a, b)| (a, Some(b)));
    !main.is_empty()
        && main.len() <= 32
        && main.split('.').all(|p| !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
        && rev.is_none_or(|r| !r.is_empty() && r.len() <= 9 && r.bytes().all(|b| b.is_ascii_digit()))
}

/// (numéros sans zéros finaux, révision) : `0.1` = `0.1.0`, et la révision ne départage que deux versions égales.
fn parts(v: &str) -> (Vec<u64>, u64) {
    let (main, rev) = v.split_once('-').map_or((v, "0"), |(a, b)| (a, b));
    let mut p: Vec<u64> = main.split('.').map(|x| x.parse().unwrap_or(0)).collect();
    while p.len() > 1 && p.last() == Some(&0) {
        p.pop();
    }
    (p, rev.parse().unwrap_or(0))
}

/// `candidate` est strictement plus récente que `current` (jamais de retour en arrière).
pub fn is_newer(candidate: &str, current: &str) -> bool {
    valid_version(candidate) && valid_version(current) && parts(candidate) > parts(current)
}

/// Version lue dans un nom de paquet Debian : `badgeuse-kiosk_0.2.0-1_arm64.deb` → `0.2.0-1`.
pub fn version_from_deb_name(file_name: &str) -> Option<String> {
    let stem = file_name.strip_suffix(".deb")?;
    let mut it = stem.split('_');
    let (name, version) = (it.next()?, it.next()?);
    (name == "badgeuse-kiosk" && valid_version(version)).then(|| version.to_string())
}

/// Parmi des noms de fichiers, le paquet de borne de version la plus récente : `(version, nom)`.
/// Sert à trouver le paquet embarqué dans l'appli par la CI.
pub fn newest_package<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<(String, &'a str)> {
    names
        .into_iter()
        .filter_map(|n| version_from_deb_name(n).map(|v| (v, n)))
        .reduce(|best, cand| if is_newer(&cand.0, &best.0) { cand } else { best })
}

// ---- Stockage côté appli (table des réglages) ----

const SECRET_KEY: &str = "kiosk_update_secret";
const OFFER_KEY: &str = "kiosk_update_offer";

/// Clé publique à copier dans la configuration des bornes, si une paire existe.
pub fn public_key(c: &rusqlite::Connection) -> Result<Option<String>> {
    crate::settings::get_raw(c, SECRET_KEY)?.map(|s| public_key_of(&s)).transpose()
}

/// Crée la paire de clés si elle n'existe pas. `regenerate` en refait une (les bornes devront recevoir
/// la nouvelle clé publique) et retire la mise à jour publiée, signée avec l'ancienne clé.
pub fn ensure_keys(c: &rusqlite::Connection, regenerate: bool) -> Result<String> {
    if !regenerate && let Some(public) = public_key(c)? {
        return Ok(public);
    }
    let (secret, public) = generate_keypair()?;
    crate::settings::set_raw(c, SECRET_KEY, &secret)?;
    unpublish(c)?;
    Ok(public)
}

/// Signe `data` avec la clé de l'appli et enregistre l'offre. Le fichier lui-même est rangé par l'appelant.
pub fn publish(c: &rusqlite::Connection, version: &str, data: &[u8]) -> Result<UpdateOffer> {
    if data.len() as u64 > MAX_PACKAGE_BYTES {
        return Err(Error::Invalid("Paquet trop volumineux".into()));
    }
    ensure_keys(c, false)?;
    let secret = crate::settings::get_raw(c, SECRET_KEY)?.unwrap_or_default();
    let offer = make_offer(&secret, version, data)?;
    let json = serde_json::to_string(&offer).map_err(|e| Error::Invalid(e.to_string()))?;
    crate::settings::set_raw(c, OFFER_KEY, &json)?;
    Ok(offer)
}

pub fn published(c: &rusqlite::Connection) -> Result<Option<UpdateOffer>> {
    Ok(crate::settings::get_raw(c, OFFER_KEY)?.and_then(|j| serde_json::from_str(&j).ok()))
}

pub fn unpublish(c: &rusqlite::Connection) -> Result<()> {
    c.execute("DELETE FROM settings WHERE key = ?1", [OFFER_KEY])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_offer_roundtrip_and_tampering() {
        let (secret, public) = generate_keypair().unwrap();
        assert_eq!(public_key_of(&secret).unwrap(), public);
        let deb = b"contenu du paquet".to_vec();
        let offer = make_offer(&secret, "0.2.0-1", &deb).unwrap();
        verify_offer(&public, &offer).unwrap();
        verify_package(&offer, &deb).unwrap();

        // Paquet modifié, tronqué : refusé.
        assert!(verify_package(&offer, b"contenu du paquet!").is_err());
        assert!(verify_package(&offer, &deb[..3]).is_err());
        // Offre falsifiée (version, empreinte, taille) : la signature ne correspond plus.
        for forged in [
            UpdateOffer { version: "9.9.9".into(), ..offer.clone() },
            UpdateOffer { sha256: sha256_hex(b"autre"), ..offer.clone() },
            UpdateOffer { size: offer.size + 1, ..offer.clone() },
            UpdateOffer { signature: "00".repeat(64), ..offer.clone() },
            UpdateOffer { signature: "zz".into(), ..offer.clone() },
        ] {
            assert!(verify_offer(&public, &forged).is_err(), "{forged:?}");
        }
        // Signée par une autre clé : refusée.
        let (_, other_public) = generate_keypair().unwrap();
        assert!(verify_offer(&other_public, &offer).is_err());
        assert!(verify_offer("pas-une-cle", &offer).is_err());
    }

    #[test]
    fn publish_flow_in_database() {
        let c = crate::db::open_in_memory().unwrap();
        assert!(public_key(&c).unwrap().is_none() && published(&c).unwrap().is_none());
        let public = ensure_keys(&c, false).unwrap();
        assert_eq!(ensure_keys(&c, false).unwrap(), public, "la clé ne change pas sans demande");
        let offer = publish(&c, "0.2.0", b"deb").unwrap();
        assert_eq!(published(&c).unwrap().as_ref(), Some(&offer));
        verify_offer(&public, &offer).unwrap();
        assert!(publish(&c, "0.2.0;id", b"deb").is_err());
        // Nouvelle clé : l'ancienne offre n'est plus proposée.
        let new_public = ensure_keys(&c, true).unwrap();
        assert_ne!(new_public, public);
        assert!(published(&c).unwrap().is_none());
        unpublish(&c).unwrap();
    }

    #[test]
    fn versions() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0-5"));
        assert!(is_newer("0.1.0-2", "0.1.0-1"));
        assert!(is_newer("0.1.0-1", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("abc", "0.1.0") && !is_newer("0.2.0", "abc"));
        assert!(valid_version("1.2.3-4") && !valid_version("1.2.3-") && !valid_version("1..2") && !valid_version("1.2~rc1"));
        assert!(!valid_version("1.0\n$(rm -rf /)"));
    }

    #[test]
    fn version_in_file_name() {
        assert_eq!(version_from_deb_name("badgeuse-kiosk_0.2.0-1_arm64.deb").as_deref(), Some("0.2.0-1"));
        assert_eq!(version_from_deb_name("badgeuse-kiosk_0.3.0_arm64.deb").as_deref(), Some("0.3.0"));
        assert_eq!(version_from_deb_name("autre_0.3.0_arm64.deb"), None);
        assert_eq!(version_from_deb_name("badgeuse-kiosk_arm64.deb"), None);
        assert_eq!(version_from_deb_name("badgeuse-kiosk_0.3.0_arm64.zip"), None);
    }

    #[test]
    fn newest_bundled_package() {
        let names = [".gitkeep", "badgeuse-kiosk_0.9.0_arm64.deb", "badgeuse-kiosk_0.10.0_arm64.deb", "autre_9.0.0_arm64.deb"];
        assert_eq!(newest_package(names), Some(("0.10.0".into(), "badgeuse-kiosk_0.10.0_arm64.deb")));
        assert_eq!(newest_package([".gitkeep"]), None);
        assert_eq!(newest_package([]), None);
    }
}
