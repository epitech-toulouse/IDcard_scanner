//! Lecteur simulé : les cartes sont « posées » via [`MockHandle::inject`].

use super::{CardInfo, CardReader, NfcError};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

pub struct MockReader {
    rx: Receiver<CardInfo>,
}

#[derive(Clone)]
pub struct MockHandle {
    tx: Sender<CardInfo>,
}

impl MockReader {
    pub fn new() -> (MockReader, MockHandle) {
        let (tx, rx) = channel();
        (MockReader { rx }, MockHandle { tx })
    }
}

impl MockHandle {
    pub fn inject(&self, uid: Vec<u8>) {
        let _ = self.tx.send(CardInfo::new(uid));
    }
}

impl CardReader for MockReader {
    fn name(&self) -> String {
        "Simulé (BADGEUSE_READER=mock)".into()
    }
    fn wait_card(&mut self, timeout: Duration) -> Result<Option<CardInfo>, NfcError> {
        match self.rx.recv_timeout(timeout) {
            Ok(c) => Ok(Some(c)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(NfcError::Backend("simulateur arrêté".into())),
        }
    }
}
