//! Mise à jour de la borne depuis l'appli Badgeuse.
//!
//! Deux étages, de privilèges différents :
//! 1. le service de la borne (utilisateur `badgeuse-kiosk`, sans droits) télécharge le paquet proposé par l'appli,
//!    contrôle signature + empreinte, puis le dépose dans `update_dir` ;
//! 2. un service ponctuel **root** (`badgeuse-kiosk-update.service`, déclenché par une unité `.path`) relance
//!    `badgeuse-kiosk --apply-update` : il REVÉRIFIE tout (le dossier est écrit par l'utilisateur de la borne, donc
//!    jamais cru sur parole), recopie le paquet dans un dossier réservé à root, puis l'installe avec `dpkg`.
//! Seule la clé publique inscrite dans `/etc/badgeuse-kiosk.toml` fait foi : le jeton réseau ne suffit pas.

use badgeuse_core::update::{self, UpdateOffer};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Version du binaire en cours d'exécution.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub const PENDING_DEB: &str = "pending.deb";
/// Écrit en dernier : sa présence déclenche l'installation.
pub const PENDING_JSON: &str = "pending.json";

#[derive(Debug, Clone)]
pub struct UpdateConfig {
    pub public_key: String,
    pub dir: PathBuf,
}

/// Faut-il télécharger cette offre ? Signée par notre clé, plus récente que la version installée, taille raisonnable.
pub fn wanted(public_key: &str, offer: &UpdateOffer, current: &str) -> Result<bool, String> {
    update::verify_offer(public_key, offer).map_err(|e| e.to_string())?;
    if offer.size == 0 || offer.size > update::MAX_PACKAGE_BYTES {
        return Err(format!("taille de paquet refusée : {} octets", offer.size));
    }
    Ok(update::is_newer(&offer.version, current))
}

/// Une mise à jour est déjà déposée, en attente d'installation.
pub fn is_staged(dir: &Path) -> bool {
    dir.join(PENDING_JSON).exists()
}

/// Dépose le paquet (déjà vérifié). Écriture dans un fichier temporaire puis renommage : l'installateur
/// ne voit jamais un paquet à moitié écrit.
pub fn stage(dir: &Path, offer: &UpdateOffer, data: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let write = |name: &str, bytes: &[u8]| -> std::io::Result<()> {
        let tmp = dir.join(format!("{name}.tmp"));
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, dir.join(name))
    };
    write(PENDING_DEB, data)?;
    write(PENDING_JSON, serde_json::to_string(offer).map_err(std::io::Error::other)?.as_bytes())
}

fn read_regular(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{} : {e}", path.display()))?;
    if !meta.is_file() || meta.len() > max {
        return Err(format!("{} : fichier refusé (lien, dossier ou trop gros)", path.display()));
    }
    fs::read(path).map_err(|e| format!("{} : {e}", path.display()))
}

/// Étage root : vérifie ce qui a été déposé dans `dir`, le recopie dans `private_dir` (inaccessible à
/// l'utilisateur de la borne, donc non modifiable entre la vérification et l'installation) et appelle
/// `install`. Les fichiers déposés sont toujours supprimés, succès ou non. Renvoie la version installée.
pub fn apply(
    public_key: &str,
    dir: &Path,
    private_dir: &Path,
    current: &str,
    install: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<String, String> {
    let result = (|| {
        let offer: UpdateOffer = serde_json::from_slice(&read_regular(&dir.join(PENDING_JSON), 4096)?)
            .map_err(|e| format!("{PENDING_JSON} illisible : {e}"))?;
        if !wanted(public_key, &offer, current)? {
            return Err(format!("version {} pas plus récente que {current} : ignorée", offer.version));
        }
        let data = read_regular(&dir.join(PENDING_DEB), update::MAX_PACKAGE_BYTES)?;
        update::verify_package(&offer, &data).map_err(|e| e.to_string())?;

        fs::create_dir_all(private_dir).map_err(|e| format!("{} : {e}", private_dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(private_dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        }
        let copy = private_dir.join("badgeuse-kiosk.deb");
        fs::write(&copy, &data).map_err(|e| format!("{} : {e}", copy.display()))?;
        let outcome = install(&copy);
        let _ = fs::remove_file(&copy);
        outcome.map(|()| offer.version)
    })();
    for name in [PENDING_JSON, PENDING_DEB] {
        let _ = fs::remove_file(dir.join(name));
    }
    result
}

/// Installation réelle : `dpkg -i`, en gardant la configuration locale (jeton, adresse).
pub fn dpkg_install(deb: &Path) -> Result<(), String> {
    let out = std::process::Command::new("dpkg")
        .args(["-i", "--force-confold"])
        .arg(deb)
        .output()
        .map_err(|e| format!("dpkg : {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("dpkg a échoué : {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("badgeuse-upd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn wanted_requires_valid_signature_and_newer_version() {
        let (secret, public) = update::generate_keypair().unwrap();
        let offer = update::make_offer(&secret, "0.2.0", b"deb").unwrap();
        assert_eq!(wanted(&public, &offer, "0.1.0"), Ok(true));
        assert_eq!(wanted(&public, &offer, "0.2.0"), Ok(false));
        assert_eq!(wanted(&public, &offer, "0.3.0"), Ok(false), "jamais de retour en arrière");
        let (_, other) = update::generate_keypair().unwrap();
        assert!(wanted(&other, &offer, "0.1.0").is_err(), "clé inconnue");
        let forged = UpdateOffer { version: "9.0.0".into(), ..offer };
        assert!(wanted(&public, &forged, "0.1.0").is_err());
    }

    #[test]
    fn apply_installs_verified_package_and_cleans_up() {
        let (secret, public) = update::generate_keypair().unwrap();
        let (dir, private) = (temp("ok"), temp("ok-private"));
        let offer = update::make_offer(&secret, "0.2.0", b"paquet").unwrap();
        stage(&dir, &offer, b"paquet").unwrap();
        assert!(is_staged(&dir));
        let seen = std::cell::RefCell::new(Vec::new());
        let v = apply(&public, &dir, &private, "0.1.0", |p| {
            *seen.borrow_mut() = fs::read(p).unwrap();
            Ok(())
        });
        assert_eq!(v, Ok("0.2.0".into()));
        assert_eq!(*seen.borrow(), b"paquet");
        assert!(!is_staged(&dir) && !dir.join(PENDING_DEB).exists());
        assert!(!private.join("badgeuse-kiosk.deb").exists());
    }

    #[test]
    fn apply_refuses_tampered_or_unsigned_packages() {
        let (secret, public) = update::generate_keypair().unwrap();
        let installed = std::cell::Cell::new(false);
        let run = |dir: &Path, key: &str, current: &str| {
            apply(key, dir, &temp("p"), current, |_| {
                installed.set(true);
                Ok(())
            })
        };
        let offer = update::make_offer(&secret, "0.2.0", b"paquet").unwrap();

        // Paquet remplacé après coup (par l'utilisateur de la borne, par exemple).
        let dir = temp("tamper");
        stage(&dir, &offer, b"paquet").unwrap();
        fs::write(dir.join(PENDING_DEB), b"malveillant").unwrap();
        assert!(run(&dir, &public, "0.1.0").is_err());
        assert!(!is_staged(&dir), "le dépôt refusé est supprimé");

        // Signé par une autre clé.
        let (other_secret, _) = update::generate_keypair().unwrap();
        let dir = temp("otherkey");
        stage(&dir, &update::make_offer(&other_secret, "0.2.0", b"paquet").unwrap(), b"paquet").unwrap();
        assert!(run(&dir, &public, "0.1.0").is_err());

        // Pas plus récent.
        let dir = temp("old");
        stage(&dir, &offer, b"paquet").unwrap();
        assert!(run(&dir, &public, "0.2.0").is_err());

        // Lien symbolique à la place du paquet.
        #[cfg(unix)]
        {
            let dir = temp("link");
            stage(&dir, &offer, b"paquet").unwrap();
            fs::remove_file(dir.join(PENDING_DEB)).unwrap();
            std::os::unix::fs::symlink("/etc/hostname", dir.join(PENDING_DEB)).unwrap();
            assert!(run(&dir, &public, "0.1.0").is_err());
        }
        assert!(!installed.get(), "rien ne doit être installé");
    }

    #[test]
    fn failed_install_reports_error_and_cleans_up() {
        let (secret, public) = update::generate_keypair().unwrap();
        let dir = temp("fail");
        stage(&dir, &update::make_offer(&secret, "0.2.0", b"x").unwrap(), b"x").unwrap();
        let r = apply(&public, &dir, &temp("fail-p"), "0.1.0", |_| Err("dépendance manquante".into()));
        assert_eq!(r, Err("dépendance manquante".into()));
        assert!(!is_staged(&dir));
    }
}
