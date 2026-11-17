//! Backend PC/SC (pcscd + libccid).

use super::{CardInfo, CardReader, Feedback, NfcError, acr122};
use pcsc::{Card, Context, Protocols, ReaderState, Scope, ShareMode, State};
use std::ffi::CString;
use std::time::Duration;

pub struct PcscReader {
    ctx: Context,
    reader: CString,
    state: State,
    card: Option<Card>,
    buzzer_configured: bool,
}

fn err(e: pcsc::Error) -> NfcError {
    NfcError::Backend(format!("PC/SC : {e}"))
}

impl PcscReader {
    pub fn open() -> Result<Self, NfcError> {
        let ctx = Context::establish(Scope::User).map_err(|e| NfcError::NoReader(format!("pcscd injoignable ({e})")))?;
        let readers = ctx.list_readers_owned().map_err(|e| NfcError::NoReader(e.to_string()))?;
        // On préfère un lecteur sans contact s'il y en a plusieurs.
        let reader = readers
            .iter()
            .find(|r| {
                let n = r.to_string_lossy().to_ascii_uppercase();
                n.contains("ACR122") || n.contains("PICC") || n.contains("CONTACTLESS")
            })
            .or(readers.first())
            .cloned()
            .ok_or_else(|| NfcError::NoReader("aucun lecteur PC/SC".into()))?;
        Ok(PcscReader { ctx, reader, state: State::UNAWARE, card: None, buzzer_configured: false })
    }

    fn read_uid(&mut self) -> Result<Option<CardInfo>, NfcError> {
        let card = match self.ctx.connect(&self.reader, ShareMode::Shared, Protocols::ANY) {
            Ok(c) => c,
            // Carte retirée entre-temps, ou carte non reconnue.
            Err(pcsc::Error::NoSmartcard | pcsc::Error::RemovedCard | pcsc::Error::UnresponsiveCard) => return Ok(None),
            Err(e) => return Err(err(e)),
        };
        let mut buf = [0u8; 64];
        let resp = match card.transmit(&acr122::GET_UID, &mut buf) {
            Ok(r) => r.to_vec(),
            Err(pcsc::Error::RemovedCard) => return Ok(None),
            Err(e) => return Err(err(e)),
        };
        if resp.len() < 3 || resp[resp.len() - 2..] != [0x90, 0x00] {
            tracing::warn!("réponse GET UID inattendue : {:02X?}", resp);
            return Ok(None);
        }
        if !self.buzzer_configured {
            let _ = card.transmit(&acr122::BUZZER_ON_DETECT_OFF, &mut buf);
            self.buzzer_configured = true;
        }
        self.card = Some(card);
        Ok(Some(CardInfo::new(resp[..resp.len() - 2].to_vec())))
    }
}

impl CardReader for PcscReader {
    fn name(&self) -> String {
        format!("PC/SC : {}", self.reader.to_string_lossy())
    }

    fn wait_card(&mut self, timeout: Duration) -> Result<Option<CardInfo>, NfcError> {
        let mut rs = [ReaderState::new(self.reader.clone(), self.state)];
        match self.ctx.get_status_change(timeout, &mut rs) {
            Ok(()) => {}
            Err(pcsc::Error::Timeout) => return Ok(None),
            Err(e) => return Err(err(e)),
        }
        let ev = rs[0].event_state();
        if ev.intersects(State::UNKNOWN | State::UNAVAILABLE) {
            return Err(NfcError::Backend("lecteur retiré".into()));
        }
        let was_present = self.state.contains(State::PRESENT);
        rs[0].sync_current_state();
        self.state = rs[0].current_state();
        let present = ev.contains(State::PRESENT) && !ev.contains(State::MUTE);
        if !present {
            self.card = None;
            return Ok(None);
        }
        if was_present {
            return Ok(None);
        }
        self.read_uid()
    }

    fn feedback(&mut self, fb: Feedback) {
        if let Some(card) = self.card.take() {
            let mut buf = [0u8; 16];
            if let Err(e) = card.transmit(&acr122::led_buzzer(fb), &mut buf) {
                tracing::debug!("retour LED ignoré : {e}");
            }
        }
    }
}
