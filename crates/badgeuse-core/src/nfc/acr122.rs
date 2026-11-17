//! Pseudo-APDU spécifiques à l'ACS ACR122U (manuel API ACR122U v2.04).

use super::Feedback;

/// Lecture de l'UID de la carte présente (PC/SC « GET DATA »).
pub const GET_UID: [u8; 5] = [0xFF, 0xCA, 0x00, 0x00, 0x00];
/// Version du firmware lecteur (réponse ASCII « ACR122U214 »).
pub const GET_FIRMWARE: [u8; 5] = [0xFF, 0x00, 0x48, 0x00, 0x00];
/// Désactive le bip automatique à la détection (on gère le retour nous-mêmes).
pub const BUZZER_ON_DETECT_OFF: [u8; 5] = [0xFF, 0x00, 0x52, 0x00, 0x00];

/// LED + bip : vert (ok), orange clignotant (retard), rouge + 3 bips (erreur).
pub fn led_buzzer(fb: Feedback) -> [u8; 9] {
    // FF 00 40 <état LED> 04 <T1> <T2> <répétitions> <bip>
    let (state, t1, t2, reps) = match fb {
        Feedback::Ok => (0x2E, 0x01, 0x01, 0x01),
        Feedback::Warn => (0xFF, 0x02, 0x02, 0x02),
        Feedback::Error => (0x5D, 0x02, 0x02, 0x03),
    };
    [0xFF, 0x00, 0x40, state, 0x04, t1, t2, reps, 0x01]
}
