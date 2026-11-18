//! Diagnostic d'une carte pour l'écran « Test cartes » : type probable, ATR, SAK, utilisabilité.
//! Fonctions pures, sans accès au lecteur ni à la base.

use super::CardInfo;
use crate::model::Student;
use chrono::NaiveDateTime;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CardDiagnostic {
    /// UID au format canonique « 04:24:A5:82 ».
    pub uid: String,
    pub uid_len: usize,
    pub sak: Option<u8>,
    /// ATR en hexadécimal (« 3B 8F 80 … »), si connu.
    pub atr: Option<String>,
    /// Type de carte probable (« MIFARE DESFire », « NTAG / Ultralight »…).
    pub card_type: String,
    /// UID aléatoire : la carte ne peut pas servir à l'émargement.
    pub random_uid: bool,
    /// Identifiant du lecteur qui a lu la carte.
    pub reader: String,
    /// Étudiant associé à cette carte, s'il y en a un.
    pub student: Option<Student>,
    pub scanned_at: NaiveDateTime,
}

impl CardDiagnostic {
    pub fn new(card: &CardInfo, reader: &str, student: Option<Student>, now: NaiveDateTime) -> Self {
        CardDiagnostic {
            uid: card.uid_hex(),
            uid_len: card.uid.len(),
            sak: card.sak,
            atr: card.atr.as_ref().map(|a| a.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")),
            card_type: card_type(card),
            random_uid: card.is_random_uid(),
            reader: reader.into(),
            student,
            scanned_at: now,
        }
    }
}

/// Type probable d'après l'ATR PC/SC (prioritaire), sinon le SAK, sinon la longueur de l'UID.
pub fn card_type(card: &CardInfo) -> String {
    card.atr
        .as_deref()
        .and_then(type_from_atr)
        .or_else(|| card.sak.and_then(type_from_sak))
        .map(str::to_string)
        .unwrap_or_else(|| match card.uid.len() {
            4 => "ISO 14443-A (UID 4 octets)".into(),
            7 => "ISO 14443-A (UID 7 octets)".into(),
            10 => "ISO 14443-A (UID 10 octets)".into(),
            n => format!("Inconnu (UID {n} octets)"),
        })
}

/// ATR PC/SC des cartes mémoire (PC/SC part 3) : `3B 8F 80 01 80 4F 0C A0 00 00 03 06 SS NN NN …`.
/// Les cartes ISO 14443-4 (DESFire, cartes à puce) ont un ATR court `3B 8x 80 01 …`.
fn type_from_atr(atr: &[u8]) -> Option<&'static str> {
    const RID: [u8; 5] = [0xA0, 0x00, 0x00, 0x03, 0x06];
    if atr.len() >= 15 && atr[0] == 0x3B && atr[7..12] == RID {
        return Some(match (atr[13], atr[14]) {
            (0x00, 0x01) => "MIFARE Classic 1K",
            (0x00, 0x02) => "MIFARE Classic 4K",
            (0x00, 0x03) => "MIFARE Ultralight / NTAG",
            (0x00, 0x26) => "MIFARE Mini",
            (0x00, 0x3A) => "MIFARE Ultralight C",
            (0x00, 0x36) => "MIFARE Plus SL1 2K",
            (0x00, 0x37) => "MIFARE Plus SL1 4K",
            (0xF0, 0x04) => "Topaz / Jewel",
            (0xF0, 0x11) => "FeliCa 212K",
            (0xF0, 0x12) => "FeliCa 424K",
            _ => "Carte mémoire ISO 14443",
        });
    }
    if atr == [0x3B, 0x81, 0x80, 0x01, 0x80, 0x80] {
        return Some("MIFARE DESFire");
    }
    if atr.len() >= 4 && atr[0] == 0x3B && atr[1] & 0xF0 == 0x80 && atr[2] == 0x80 && atr[3] == 0x01 {
        return Some("ISO 14443-4 (carte à puce)");
    }
    None
}

fn type_from_sak(sak: u8) -> Option<&'static str> {
    Some(match sak {
        0x00 => "MIFARE Ultralight / NTAG",
        0x08 | 0x88 => "MIFARE Classic 1K",
        0x09 => "MIFARE Mini",
        0x10 | 0x11 => "MIFARE Plus 2K/4K",
        0x18 => "MIFARE Classic 4K",
        0x20 => "MIFARE DESFire / ISO 14443-4",
        0x28 | 0x38 => "Carte à puce (émulation MIFARE)",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(uid: &[u8], sak: Option<u8>, atr: Option<&[u8]>) -> CardInfo {
        CardInfo { uid: uid.to_vec(), sak, atr: atr.map(<[u8]>::to_vec) }
    }

    #[test]
    fn type_from_pcsc_atr() {
        let classic = [0x3B, 0x8F, 0x80, 0x01, 0x80, 0x4F, 0x0C, 0xA0, 0x00, 0x00, 0x03, 0x06, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x6A];
        assert_eq!(card_type(&card(&[1, 2, 3, 4], None, Some(&classic))), "MIFARE Classic 1K");
        let ntag = [0x3B, 0x8F, 0x80, 0x01, 0x80, 0x4F, 0x0C, 0xA0, 0x00, 0x00, 0x03, 0x06, 0x03, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x68];
        assert_eq!(card_type(&card(&[4, 1, 2, 3, 4, 5, 6], None, Some(&ntag))), "MIFARE Ultralight / NTAG");
        let desfire = [0x3B, 0x81, 0x80, 0x01, 0x80, 0x80];
        assert_eq!(card_type(&card(&[4, 1, 2, 3, 4, 5, 6], None, Some(&desfire))), "MIFARE DESFire");
        let iso4 = [0x3B, 0x88, 0x80, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0x09];
        assert_eq!(card_type(&card(&[8, 1, 2, 3], None, Some(&iso4))), "ISO 14443-4 (carte à puce)");
    }

    #[test]
    fn type_from_sak_then_uid_len() {
        assert_eq!(card_type(&card(&[4, 0x24, 0xA5, 0x82, 0xE6, 0x19, 0x90], Some(0x20), None)), "MIFARE DESFire / ISO 14443-4");
        assert_eq!(card_type(&card(&[1, 2, 3, 4], Some(0x18), None)), "MIFARE Classic 4K");
        assert_eq!(card_type(&card(&[1, 2, 3, 4], Some(0x42), None)), "ISO 14443-A (UID 4 octets)");
        assert_eq!(card_type(&card(&[1, 2], None, None)), "Inconnu (UID 2 octets)");
    }

    #[test]
    fn diagnostic_fields() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 17).unwrap().and_hms_opt(9, 0, 0).unwrap();
        let d = CardDiagnostic::new(&card(&[0x08, 1, 2, 3], None, Some(&[0x3B, 0x81, 0x80, 0x01, 0x80, 0x80])), "mock", None, now);
        assert_eq!(d.uid, "08:01:02:03");
        assert_eq!(d.uid_len, 4);
        assert!(d.random_uid);
        assert_eq!(d.atr.as_deref(), Some("3B 81 80 01 80 80"));
    }
}
