//! Pseudo-APDU spécifiques à l'ACS ACR122U (manuel API ACR122U v2.04).

use super::{Beep, Feedback};

/// Lecture de l'UID de la carte présente (PC/SC « GET DATA »).
pub const GET_UID: [u8; 5] = [0xFF, 0xCA, 0x00, 0x00, 0x00];
/// Version du firmware lecteur (réponse ASCII « ACR122U214 »).
pub const GET_FIRMWARE: [u8; 5] = [0xFF, 0x00, 0x48, 0x00, 0x00];
/// Désactive le bip automatique à la détection (on gère le retour nous-mêmes).
pub const BUZZER_ON_DETECT_OFF: [u8; 5] = [0xFF, 0x00, 0x52, 0x00, 0x00];

/// LED + bip : vert (ok), orange clignotant (retard), rouge clignotant (erreur).
/// Le volume du buzzer n'est pas réglable : `Beep` joue sur le nombre et la durée des bips.
/// - `Normal`  : 1 bip court (ok), 2 bips (retard), 3 bips (erreur) ;
/// - `Discreet`: un seul bip de 100 ms dans tous les cas, LED allumée une fois ;
/// - `Silent`  : LED seules.
pub fn led_buzzer(fb: Feedback, beep: Beep) -> [u8; 9] {
    // FF 00 40 <état LED> 04 <T1> <T2> <répétitions> <bip : 01 = pendant T1, 00 = aucun>
    let (state, t1, t2, reps) = match fb {
        Feedback::Ok => (0x2E, 0x01, 0x01, 0x01),
        // Déjà validé : LED verte seule, jamais de bip quel que soit le mode.
        Feedback::Silent => return [0xFF, 0x00, 0x40, 0x2E, 0x04, 0x01, 0x01, 0x01, 0x00],
        Feedback::Warn => (0xFF, 0x02, 0x02, 0x02),
        Feedback::Error => (0x5D, 0x02, 0x02, 0x03),
    };
    match beep {
        Beep::Normal => [0xFF, 0x00, 0x40, state, 0x04, t1, t2, reps, 0x01],
        Beep::Discreet => [0xFF, 0x00, 0x40, state, 0x04, 0x01, 0x01, 0x01, 0x01],
        Beep::Silent => [0xFF, 0x00, 0x40, state, 0x04, t1, t2, reps, 0x00],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beep_modes() {
        assert_eq!(led_buzzer(Feedback::Error, Beep::Normal)[5..], [0x02, 0x02, 0x03, 0x01]);
        assert_eq!(led_buzzer(Feedback::Error, Beep::Discreet)[5..], [0x01, 0x01, 0x01, 0x01]);
        assert_eq!(led_buzzer(Feedback::Error, Beep::Silent)[5..], [0x02, 0x02, 0x03, 0x00]);
        assert_eq!(led_buzzer(Feedback::Silent, Beep::Normal)[8], 0x00);
        assert_eq!(led_buzzer(Feedback::Silent, Beep::Normal)[3], led_buzzer(Feedback::Ok, Beep::Normal)[3]);
        // La couleur de LED ne dépend pas du mode de bip.
        assert_eq!(led_buzzer(Feedback::Warn, Beep::Discreet)[3], led_buzzer(Feedback::Warn, Beep::Normal)[3]);
    }
}
