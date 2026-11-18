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
src-tauri/src/
  lib.rs         état (Mutex<Connection>, créneau actif), un thread par lecteur + superviseur PC/SC → événements
  commands.rs    #[tauri::command] fins qui délèguent au core
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

Tout reste en local, sans aucun appel réseau : ne pas en ajouter (télémétrie, CDN, polices distantes…).
Les sauvegardes et les exports se font à la main par l'utilisateur. Purge de fin d'année : supprimer la promotion (cascade).

## Tests manuels

- Lecteur simulé : `BADGEUSE_READER=mock`, import de `fixtures/etudiants.csv`, création d'un créneau, puis badges avec des UID inventés.
- Vrai lecteur : la carte de test connue a l'UID `04:24:A5:82:E6:19:90` (DESFire, SAK 0x20).
