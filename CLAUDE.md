# Badgeuse

Application de bureau Linux pour faire l'appel des étudiants avec leur carte étudiant NFC.
Elle gère les créneaux d'émargement, les groupes (cursus > promotion > groupe) et l'export Excel / CSV des présences.
Utilisateur cible : un enseignant, sur son PC, avec un lecteur ACS ACR122U.

## Stack

- **Tauri 2** : backend Rust dans `src-tauri/`, interface Svelte 5 + TypeScript + Vite dans `src/`.
- **Logique métier** : crate `crates/badgeuse-core`, indépendante de Tauri.
  - Base : SQLite via `rusqlite` (bundled).
  - Excel : `rust_xlsxwriter` pour l'export, `calamine` + `csv` pour l'import.
  - NFC : `pcsc` (feature `pcsc`) et `rusb` (feature `usb`, libusb vendored).
- **Base** : `~/.local/share/fr.badgeuse.app/badgeuse.db`.

## Commandes

```bash
npm install                                   # une fois
npm run tauri dev                             # lance l'appli (vrai lecteur, détection auto)
BADGEUSE_READER=mock npm run tauri dev        # lecteur simulé : champ « Badger » sur l'écran d'émargement
cargo test -p badgeuse-core                   # tests métier (pas besoin de webkit ni pcsclite)
npm run check                                 # svelte-check (types TS + Svelte)
npm test                                      # vitest (src/**/*.test.ts)
npm run tauri build                           # .deb + AppImage dans target/release/bundle/
python3 tools/acr122u_test.py 30              # diagnostic du lecteur en USB direct (sans pcscd)
python3 tools/carte_tool.py                   # testeur PC/SC : lecteur + UID/ATR + contenu de la carte
python3 tools/carte_tool.py read 4            # lit un bloc/une page
python3 tools/carte_tool.py write 6 DEADBEEF  # écrit (édite) une carte MIFARE Classic / Ultralight / NTAG
cargo test -p badgeuse-kiosk                  # borne Raspberry Pi : file d'attente, envoi, affichage
cargo deb -p badgeuse-kiosk                   # paquet .deb de la borne (arm64 sur un Pi / runner arm64)
```

Prérequis système (Ubuntu/Debian) :
`sudo apt install libwebkit2gtk-4.1-dev libpcsclite-dev pcscd libccid pcsc-tools`.
Le pilote noyau `pn533_usb` capture l'ACR122U : il faut le blacklister pour PC/SC
(`/etc/modprobe.d/blacklist-acr122u.conf` → `blacklist pn533_usb`). Le backend USB direct le détache lui-même.

## Architecture

```
crates/badgeuse-core/src/
  model.rs       types sérialisés vers le front (miroir : src/lib/types.ts)
  domain.rs      règles pures : statut présent/retard, comptage, taux
  db.rs          ouverture, migrations (PRAGMA user_version), sauvegarde/restauration
  groups.rs students.rs sessions.rs attendance.rs settings.rs   accès données par entité
  import.rs      CSV (UTF-8/Windows-1252, ; , tab) et XLSX/XLS/ODS → étudiants
  export.rs      xlsx ou CSV (selon l'extension ; CSV = « ; » + BOM UTF-8) : feuille d'un créneau, et période
                 (matrice + statistiques ; le CSV n'a pas les statistiques par groupe)
  nfc/           trait CardReader, run_loop (reconnexion, anti-rebond 3 s), pcsc.rs, usb.rs, mock.rs, acr122.rs,
                 diagnostic.rs (type de carte d'après ATR/SAK, pour « Test cartes »)
  update.rs      mise à jour des bornes : SHA-256, signature Ed25519, comparaison de versions, clés/offre en base
  remote.rs      protocole borne ↔ appli (RemoteScan, RemoteReply, HeartbeatReply) et règles pures : correction
                 d'horloge, jeton (comparaison en temps constant), réponse minimale pour l'écran de la borne
crates/badgeuse-kiosk/   borne de scan (Raspberry Pi) : binaire séparé, sans Tauri
  config.rs      /etc/badgeuse-kiosk.toml (validé : HTTP seulement, jeton obligatoire, affichage en 127.0.0.1)
  queue.rs       file SQLite des badges non transmis (écrits AVANT l'envoi, supprimés une fois traités)
  updater.rs     mise à jour : la borne dépose un paquet vérifié ; `--apply-update` (service root) le revérifie et lance dpkg
  client.rs      client HTTP (trait Transport, doublé dans les tests) ; kiosk.rs : on_card + flush (toutes les 10 s)
  display.rs web.rs static/index.html   page plein écran servie sur 127.0.0.1:8080 (interrogation de /state, aucune ressource distante)
  readers.rs     lecteurs PC/SC (un thread par lecteur) ou simulé ; dist/ : unités systemd, polkit, scripts du .deb
src-tauri/src/
  lib.rs         état (Mutex<Connection>, créneau actif), un thread par lecteur + superviseur PC/SC → événements
  commands.rs    #[tauri::command] fins qui délèguent au core
  remote_server.rs  serveur HTTP (axum) des bornes : POST /api/scan et /api/heartbeat, jeton Bearer, → card-scanned
src/
  lib/api.ts     wrappers typés des commandes et des événements
  lib/app.svelte.ts   route (#/page/id), statut lecteur, toasts, dialogues natifs
  pages/         Accueil, Emargement (écran live + projection), Affichage (plein écran : créneau actif jusqu'à début + retard, puis « prochain émargement à … »), Creneaux, Etudiants, Groupes, Exports, Cartes (test), Reglages
  components/    Modal, TargetPicker, StudentPicker, Projection, SessionForm, StudentForm, ImportWizard…
```

Flux d'un badge :
1. Le thread `nfc-reader` détecte la carte.
2. `on_card` (lib.rs) appelle `attendance::record_scan`, ou `identify` si aucun créneau n'est actif.
3. L'événement `card-scanned` (un `ScanOutcome`) part vers le front, avec le retour LED/bip.
4. Une carte inconnue ouvre `StudentPicker`, puis `link_card` puis `rescan`.

Mode « Test cartes » (`set_card_test`, actif tant que la page `#/cartes` est affichée) : `on_card` émet
`card-test` (un `CardDiagnostic`) au lieu d'enregistrer quoi que ce soit, même si un créneau est actif.

Flux d'un badge de **borne** (réglage `remote_enabled`, désactivé par défaut ; activation et port pris en compte au démarrage) :
1. La borne écrit le badge dans sa file locale, puis l'envoie (`POST /api/scan`, `RemoteScan` avec `scan_id` unique).
2. `remote_server.rs` vérifie le jeton, puis `attendance::record_remote_scan` : même scan_id = même réponse (table `remote_scans`,
   purgée à 30 jours) ; heure corrigée de l'écart d'horloge de la borne (le Pi n'a pas de pile) ; créneau = l'actif si la
   fenêtre du badge le contient, sinon le créneau ouvert **ou clos** de cette fenêtre où l'étudiant est attendu
   (`domain::pick_remote_session`). Un badge rejoué après la clôture remplace l'absence `auto`.
3. L'événement `card-scanned` part vers le front comme pour un badge local ; la borne reçoit une `RemoteReply` minimale.
4. Hors ligne, la borne garde le badge et le renvoie dans l'ordre (toutes les 10 s) avec son heure d'origine.
   En mode « Test cartes », un badge rejoué (`RemoteScan::is_replay`) est refusé en 503 pour que la borne le garde.
   Une borne apparaît dans `ReadersStatus` (`backend: "remote"`, nom `borne:<device>`), hors ligne après 30 s sans signal.

**Mise à jour des bornes** (Réglages → Bornes réseau → Mise à jour) : l'enseignant publie un `badgeuse-kiosk_<version>_arm64.deb` ;
l'appli le range dans `<données>/kiosk-update/`, le signe (Ed25519, clé privée en base, jamais envoyée) et l'annonce dans la réponse au
signal de vie (`HeartbeatReply.update`) ; la borne le télécharge (`GET /api/update/package`, jeton) si elle est au repos (file vide,
aucun badge depuis 60 s), contrôle signature + empreinte avec `update_public_key` (config de la borne, vide = désactivé) et le dépose
dans `/var/lib/badgeuse-kiosk/update/`. `badgeuse-kiosk-update.path` déclenche alors `badgeuse-kiosk-update.service` (root) :
`--apply-update` revérifie tout, copie dans `/run/badgeuse-kiosk-update` (0700) puis `dpkg -i --force-confold`. Jamais de version
plus ancienne ; le jeton seul ne permet pas de pousser un paquet (il circule en clair sur le LAN).

## Vocabulaire et règles métier

| UI (fr) | Code (en) |
|---|---|
| cursus | program |
| promotion | cohort |
| groupe TD/TP | group |
| créneau | session |
| présence | attendance |
| absence justifiée | excused |

- Attendus d'un créneau = étudiants **actifs** des groupes ciblés ∪ des promotions ciblées (`sessions::EXPECTED_SQL`).
- Statut au badge :
  - `present` si l'heure ≤ début + `late_after_min` (borne incluse), sinon `late` ;
  - un nouveau badge ne rétrograde jamais un présent/retard, mais remplace une absence.
- Clôture : les attendus sans ligne passent `absent` avec `source='auto'`. La réouverture supprime ces lignes `auto`.
- Automatique (réglages `auto_open`, `auto_open_lead_min`, `auto_close`) : un thread `sessions-auto` (lib.rs) appelle
  `attendance::run_auto` toutes les 20 s (règle pure : `domain::auto_action`). Un créneau `planned` s'ouvre de
  `début − avance` jusqu'à sa fin (jamais après coup). À chaque passage, le créneau actif est réévalué (`domain::pick_active`) :
  il bascule sur le créneau ouvert en cours dès que l'actif est terminé (événement `active-session`) ;
  un créneau ouvert dont la fin est passée est clos si `auto_close` (défaut : non). Un créneau clos automatiquement
  puis rouvert à la main n'est pas reclos (mémoire du thread). Événement front : `session-auto`.
- Sources : `badge`, `manual` (correction du prof), `auto` (clôture).
- UID stocké au format `04:24:A5:82:E6:19:90` (`nfc::format_uid`). Les UID aléatoires (4 octets commençant par `0x08`) sont refusés.
- Un seul créneau « actif » reçoit les badges : c'est celui ouvert ou affiché dans l'écran d'émargement.
- Multi-lecteurs : en PC/SC, un thread par lecteur (clé = nom PC/SC) ; un superviseur détecte les lecteurs branchés en cours de route. Tous alimentent le même créneau actif. L'anti-rebond de 3 s est par lecteur. L'USB direct et le simulé restent mono-lecteur. Le front reçoit une vue agrégée `ReadersStatus` (événement `reader-status`).

## Conventions

- Logique métier dans `badgeuse-core` (pure si possible, dans `domain.rs`) et testée. Les commandes Tauri restent des passe-plats.
- **Schéma** : ne jamais modifier une migration existante. Ajouter `migrations/00N_xxx.sql` et l'enregistrer dans `db::MIGRATIONS`.
- Toute modification d'un type de `model.rs` doit être répercutée dans `src/lib/types.ts`.
- Arguments des commandes en camelCase côté JS (`programId`), champs des structs en snake_case.
- Code et identifiants en anglais. Interface, messages d'erreur et commentaires en français.
- Svelte 5 runes uniquement (`$state`, `$derived`, `$props`, `onclick`), pas de stores Svelte 4.
- Les erreurs Rust (`badgeuse_core::Error`) arrivent au front en chaîne : les afficher via `guard()`.

## Données personnelles (RGPD)

Tout reste en local : aucun service externe, aucune télémétrie, aucun CDN ni police distante. Le seul trafic réseau
autorisé est celui des bornes (Raspberry Pi) vers l'appli du PC de l'enseignant, **sur le réseau local** :
- l'appli ne fait que répondre (désactivé par défaut) ; jeton obligatoire, jamais de borne acceptée sans jeton ;
- HTTP en clair (pas de TLS) : le jeton circule en clair sur le LAN, ne jamais exposer le port vers Internet ;
- la borne ne reçoit que prénom + initiale du nom (jamais numéro étudiant ni e-mail) ; sa file ne contient que UID + heure
  et se vide à la transmission ; `remote_scans` (UID + heure) est purgée au bout de 30 jours.
Les sauvegardes et les exports se font à la main par l'utilisateur. Purge de fin d'année : supprimer la promotion (cascade).

## Tests manuels

- Lecteur simulé : `BADGEUSE_READER=mock`, import de `fixtures/etudiants.csv`, création d'un créneau, puis badges avec des UID inventés.
- Vrai lecteur : la carte de test connue a l'UID `04:24:A5:82:E6:19:90` (DESFire, SAK 0x20).
