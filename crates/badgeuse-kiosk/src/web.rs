//! Petit serveur web local : sert la page d'affichage et son état (`/state`) à Chromium en plein écran.
//! Uniquement sur 127.0.0.1 (voir `Config::validate`) : rien n'est exposé sur le réseau.

use crate::kiosk::Kiosk;
use badgeuse_core::nfc::{self, MockHandle};
use std::io::Read;
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server};

const PAGE: &str = include_str!("../static/index.html");

pub fn bind(listen: &str) -> Result<Server, String> {
    Server::http(listen).map_err(|e| format!("affichage : impossible d'écouter sur {listen} ({e})"))
}

/// Traite les requêtes dans un thread dédié. `mock` : lecteur simulé, qui active `/mock` et `/inject`.
pub fn spawn(server: Server, kiosk: Arc<Kiosk>, mock: Option<MockHandle>) {
    std::thread::Builder::new()
        .name("display-web".into())
        .spawn(move || {
            for req in server.incoming_requests() {
                handle(req, &kiosk, mock.as_ref());
            }
        })
        .expect("thread affichage");
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name, value).expect("en-tête valide")
}

fn handle(mut req: Request, kiosk: &Kiosk, mock: Option<&MockHandle>) {
    let path = req.url().split('?').next().unwrap_or("").to_string();
    let reply = match (req.method(), path.as_str()) {
        (Method::Get, "/") => req.respond(
            Response::from_string(PAGE).with_header(header("Content-Type", "text/html; charset=utf-8")).with_header(header("Cache-Control", "no-store")),
        ),
        (Method::Get, "/state") => req.respond(
            Response::from_string(kiosk.display.state_json()).with_header(header("Content-Type", "application/json")).with_header(header("Cache-Control", "no-store")),
        ),
        (Method::Get, "/mock") => req.respond(Response::empty(if mock.is_some() { 204 } else { 404 })),
        (Method::Post, "/inject") => {
            let code = match mock {
                None => 404,
                Some(handle) => {
                    let mut body = String::new();
                    let _ = req.as_reader().take(64).read_to_string(&mut body);
                    match nfc::parse_uid(&body) {
                        Some(uid) => {
                            handle.inject(uid);
                            204
                        }
                        None => 422,
                    }
                }
            };
            req.respond(Response::empty(code))
        }
        (Method::Get, "/favicon.ico") => req.respond(Response::empty(204)),
        _ => req.respond(Response::empty(404)),
    };
    if let Err(e) = reply {
        tracing::debug!("affichage : réponse non envoyée ({e})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{SendError, Transport};
    use crate::queue::Queue;
    use badgeuse_core::nfc::CardReader;
    use badgeuse_core::remote::{HeartbeatReply, RemoteReply, RemoteScan};
    use std::time::Duration;

    struct Offline;
    impl Transport for Offline {
        fn send_scan(&self, _: &RemoteScan) -> Result<RemoteReply, SendError> {
            Err(SendError::Retry("hors ligne".into()))
        }
        fn heartbeat(&self, _: u32) -> Result<HeartbeatReply, SendError> {
            Err(SendError::Retry("hors ligne".into()))
        }
        fn fetch_package(&self, _: u64) -> Result<Vec<u8>, SendError> {
            Err(SendError::Retry("hors ligne".into()))
        }
    }

    fn start(mock: Option<MockHandle>) -> (String, Arc<Kiosk>) {
        let kiosk = Arc::new(Kiosk::new(
            "borne1".into(),
            Queue::open_in_memory(),
            Arc::new(Offline),
            Box::new(badgeuse_core::db::now),
        ));
        let server = bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr().to_ip().unwrap());
        spawn(server, kiosk.clone(), mock);
        (url, kiosk)
    }

    fn get(url: &str) -> (u16, String) {
        let mut r = ureq::get(url).config().http_status_as_error(false).build().call().unwrap();
        (r.status().as_u16(), r.body_mut().read_to_string().unwrap())
    }

    #[test]
    fn serves_page_and_state_only() {
        let (url, kiosk) = start(None);
        let (code, html) = get(&format!("{url}/"));
        assert!(code == 200 && html.contains("Passez votre carte"));
        // Aucune ressource distante dans la page (RGPD : pas d'appel réseau tiers).
        assert!(!html.contains("http://") && !html.contains("https://"));
        let (code, json) = get(&format!("{url}/state"));
        assert_eq!(code, 200);
        assert!(json.contains("\"network\":\"unknown\""));
        kiosk.display.set_pending(2);
        assert!(get(&format!("{url}/state")).1.contains("\"pending\":2"));
        // Pas de lecteur simulé : ni /mock ni /inject.
        assert_eq!(get(&format!("{url}/mock")).0, 404);
        let r = ureq::post(format!("{url}/inject")).config().http_status_as_error(false).build().send("04:A1").unwrap();
        assert_eq!(r.status().as_u16(), 404);
        assert_eq!(get(&format!("{url}/inconnu")).0, 404);
    }

    #[test]
    fn mock_reader_can_be_driven_from_the_page() {
        let (mut reader, handle) = nfc::MockReader::new();
        let (url, _) = start(Some(handle));
        assert_eq!(get(&format!("{url}/mock")).0, 204);
        let post = |body: &str| {
            ureq::post(format!("{url}/inject")).config().http_status_as_error(false).build().send(body).unwrap().status().as_u16()
        };
        assert_eq!(post("zz"), 422);
        assert_eq!(post("04:24:A5:82"), 204);
        let card = reader.wait_card(Duration::from_secs(2)).unwrap().expect("carte injectée");
        assert_eq!(card.uid_hex(), "04:24:A5:82");
    }
}
