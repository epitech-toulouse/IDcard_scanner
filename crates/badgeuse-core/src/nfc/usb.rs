//! Backend USB direct pour ACR122U (sans pcscd) : CCID sur bulk, commandes PN532 via Escape.
//! Portage de `tools/acr122u_test.py`. Le pilote noyau `pn533_usb` est détaché pendant
//! l'utilisation puis réattaché.

use super::{Beep, CardInfo, CardReader, Feedback, NfcError, acr122};
use rusb::{DeviceHandle, GlobalContext};
use std::time::{Duration, Instant};

const VID: u16 = 0x072F;
const PID: u16 = 0x2200;
const EP_OUT: u8 = 0x02;
const EP_IN: u8 = 0x82;
const IO_TIMEOUT: Duration = Duration::from_secs(3);

/// PC_to_RDR_Escape : commande directe au lecteur, ne nécessite pas de carte.
const CCID_ESCAPE: u8 = 0x6B;

pub struct UsbReader {
    h: DeviceHandle<GlobalContext>,
    detached: bool,
    seq: u8,
    firmware: String,
    present: Option<Vec<u8>>,
}

fn err(e: rusb::Error) -> NfcError {
    NfcError::Backend(format!("USB : {e}"))
}

impl UsbReader {
    pub fn open() -> Result<Self, NfcError> {
        let h = rusb::open_device_with_vid_pid(VID, PID)
            .ok_or_else(|| NfcError::NoReader("ACR122U non branché ou accès refusé".into()))?;
        let mut detached = false;
        if h.kernel_driver_active(0).unwrap_or(false) {
            h.detach_kernel_driver(0).map_err(|e| NfcError::NoReader(format!("détache du pilote noyau impossible ({e})")))?;
            detached = true;
        }
        if let Err(e) = h.claim_interface(0) {
            if detached {
                let _ = h.attach_kernel_driver(0);
            }
            return Err(NfcError::NoReader(format!("lecteur occupé, pcscd tourne peut-être ({e})")));
        }
        let mut r = UsbReader { h, detached, seq: 0, firmware: String::new(), present: None };
        let fw = r.escape(&acr122::GET_FIRMWARE)?;
        r.firmware = String::from_utf8_lossy(&fw).trim().to_string();
        // RFConfiguration : MxRtyATR=FF, MxRtyPSL=01, MxRtyPassiveActivation=01 → poll non bloquant.
        r.pn532(&[0x32, 0x05, 0xFF, 0x01, 0x01])?;
        r.escape(&acr122::BUZZER_ON_DETECT_OFF).ok();
        Ok(r)
    }

    fn ccid(&mut self, msg_type: u8, payload: &[u8]) -> Result<Vec<u8>, NfcError> {
        let mut out = Vec::with_capacity(10 + payload.len());
        out.push(msg_type);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0, self.seq, 0, 0, 0]);
        out.extend_from_slice(payload);
        self.seq = self.seq.wrapping_add(1);
        self.h.write_bulk(EP_OUT, &out, IO_TIMEOUT).map_err(err)?;
        let mut buf = [0u8; 512];
        loop {
            let n = self.h.read_bulk(EP_IN, &mut buf, IO_TIMEOUT).map_err(err)?;
            if n < 10 {
                return Err(NfcError::Backend("réponse CCID tronquée".into()));
            }
            if buf[7] & 0xC0 == 0x80 {
                continue; // demande de délai supplémentaire
            }
            let len = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
            return Ok(buf[10..(10 + len).min(n)].to_vec());
        }
    }

    fn escape(&mut self, apdu: &[u8]) -> Result<Vec<u8>, NfcError> {
        self.ccid(CCID_ESCAPE, apdu)
    }

    /// Commande PN532 encapsulée : FF 00 00 00 Lc D4 <cmd…>.
    fn pn532(&mut self, cmd: &[u8]) -> Result<Vec<u8>, NfcError> {
        let mut apdu = vec![0xFF, 0x00, 0x00, 0x00, (cmd.len() + 1) as u8, 0xD4];
        apdu.extend_from_slice(cmd);
        self.escape(&apdu)
    }

    /// InListPassiveTarget ISO14443A : renvoie (uid, sak) si une carte est dans le champ.
    fn poll_once(&mut self) -> Result<Option<(Vec<u8>, u8)>, NfcError> {
        let r = self.pn532(&[0x4A, 0x01, 0x00])?;
        // D5 4B <nb> <Tg> <ATQA 2> <SAK> <len> <uid…>
        if r.len() >= 8 && r[0] == 0xD5 && r[1] == 0x4B && r[2] >= 1 {
            let len = r[7] as usize;
            if r.len() >= 8 + len {
                let found = (r[8..8 + len].to_vec(), r[6]);
                self.pn532(&[0x52, 0x00]).ok(); // InRelease
                return Ok(Some(found));
            }
        }
        Ok(None)
    }
}

impl CardReader for UsbReader {
    fn name(&self) -> String {
        format!("USB direct : {}", self.firmware)
    }

    fn wait_card(&mut self, timeout: Duration) -> Result<Option<CardInfo>, NfcError> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.poll_once()? {
                Some((uid, sak)) => {
                    if self.present.as_ref() != Some(&uid) {
                        self.present = Some(uid.clone());
                        return Ok(Some(CardInfo { uid, sak: Some(sak), atr: None }));
                    }
                }
                None => self.present = None,
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(120));
        }
    }

    fn feedback(&mut self, fb: Feedback, beep: Beep) {
        if let Err(e) = self.escape(&acr122::led_buzzer(fb, beep)) {
            tracing::debug!("retour LED ignoré : {e}");
        }
    }
}

impl Drop for UsbReader {
    fn drop(&mut self) {
        self.pn532(&[0x32, 0x01, 0x00]).ok(); // coupe le champ RF
        let _ = self.h.release_interface(0);
        if self.detached {
            let _ = self.h.attach_kernel_driver(0);
        }
    }
}
