# Badgeuse

Émargement des étudiants par carte étudiant NFC, pour Linux.

L'enseignant lance l'appli sur son PC, branche un lecteur ACS ACR122U, et les étudiants badgent. Une ou plusieurs **bornes**
(Raspberry Pi + lecteur + écran) peuvent aussi être posées à l'entrée de la salle : elles envoient les badges à l'appli par le
réseau local.

- **Créneaux d'émargement** : ouverture (à la main ou automatique), retard au-delà d'un seuil, corrections manuelles,
  absences justifiées, clôture (les non-badgés passent absents).
- **Écran live** et **mode projection** : le nom s'affiche en vert, orange ou rouge à chaque badge.
- **Groupes** : cursus → promotion → groupes TD/TP.
- **Import** des étudiants depuis un CSV ou un fichier Excel/ODS. Chaque carte est associée à son étudiant au premier badge.
- **Exports Excel / CSV** : feuille d'un créneau, ou récapitulatif sur une période (matrice étudiants × créneaux et statistiques).
- **Données 100 % locales** (SQLite), sauvegarde et restauration en un clic.

---

## Sommaire

1. [Installer l'appli sur le PC](#1-installer-lappli-sur-le-pc)
2. [Installer une borne (Raspberry Pi)](#2-installer-une-borne-raspberry-pi)
3. [Utilisation](#3-utilisation)
4. [Mises à jour](#4-mises-à-jour)
5. [Dépannage](#5-dépannage)
6. [Développement](#6-développement)
7. [Données personnelles et sécurité](#7-données-personnelles-et-sécurité)

---

## 1. Installer l'appli sur le PC

**Prérequis** : Ubuntu 22.04 ou plus récent (ou Debian équivalente), 64 bits (x86_64), un lecteur **ACS ACR122U** en USB.

### 1.1 Télécharger

Dans la page [Releases](https://github.com/epitech-toulouse/IDcard_scanner/releases/latest) du dépôt, prenez :

| Fichier | Pour qui |
|---|---|
| `Badgeuse_X.Y.Z_amd64.deb` | **Recommandé** : Ubuntu / Debian |
| `Badgeuse_X.Y.Z_amd64.AppImage` | Autres distributions, ou sans droits d'installation |

Le dépôt est privé : connectez-vous à GitHub dans le navigateur, ou utilisez `gh` :

```bash
gh release download --repo epitech-toulouse/IDcard_scanner --pattern 'Badgeuse_*_amd64.deb'
```

### 1.2 Installer

```bash
sudo apt install ./Badgeuse_*_amd64.deb      # installe aussi pcscd et libccid
```

> Utilisez `apt install ./…` plutôt que `dpkg -i` : apt récupère les dépendances.

Avec l'AppImage : `sudo apt install pcscd libccid`, puis `chmod +x Badgeuse_*.AppImage && ./Badgeuse_*.AppImage`.

### 1.3 Préparer le lecteur NFC (une seule fois)

Le pilote noyau `pn533_usb` s'empare de l'ACR122U et l'empêche de fonctionner en PC/SC. Il faut le bloquer :

```bash
echo "blacklist pn533_usb" | sudo tee /etc/modprobe.d/blacklist-acr122u.conf
sudo modprobe -r pn533_usb        # ou débranchez / rebranchez le lecteur
```

Vérification (facultative) : `sudo apt install pcsc-tools && pcsc_scan`, puis posez une carte : son ATR doit s'afficher.

### 1.4 Premier lancement

Lancez **Badgeuse** depuis le menu des applications. En bas de l'écran, le statut du lecteur doit passer au vert.
La base de données est créée dans `~/.local/share/fr.badgeuse.app/badgeuse.db`.

Pour **désinstaller** : `sudo apt remove badgeuse` (la base reste en place ; supprimez le dossier ci-dessus pour l'effacer).

---

## 2. Installer une borne (Raspberry Pi)

Une borne est un Raspberry Pi posé à l'entrée de la salle : l'étudiant badge, l'écran affiche « Bonjour Marie D. » et la
progression du créneau (« 10 / 24 présents »). Les badges partent vers l'appli du PC de l'enseignant, qui reste le seul tableau
de bord. Si le réseau est coupé, la borne garde les badges et les renvoie au retour, **avec leur heure d'origine**.

```
 ┌─────────── Borne ───────────┐          réseau local          ┌──── PC de l'enseignant ────┐
 │ ACR122U → badgeuse-kiosk ───┼──── HTTP + jeton (port 8737) ──▶│ Appli Badgeuse             │
 │ Écran HDMI ← cage+Chromium  │                                 │ (créneaux, exports…)       │
 └─────────────────────────────┘                                 └────────────────────────────┘
```

### 2.1 Matériel

- Raspberry Pi 4 (ou 5), alimentation officielle, carte microSD de 16 Go ou plus ;
- lecteur ACS ACR122U en USB ;
- écran HDMI (câble micro-HDMI sur le Pi 4) ;
- réseau : Wi-Fi ou Ethernet, **sur le même réseau local que le PC de l'enseignant**.

### 2.2 Activer les bornes dans l'appli (côté PC)

1. **Réglages → Bornes réseau** : cochez **Accepter les bornes réseau**, gardez le port `8737` (ou choisissez-en un autre), enregistrez.
2. **Relancez l'appli** (l'activation et le port sont pris en compte au démarrage).
3. Revenez dans **Réglages → Bornes réseau** et notez :
   - l'**adresse** affichée, par ex. `http://192.168.1.20:8737` ;
   - le **jeton** (bouton **Copier**).
4. Si un pare-feu est actif sur le PC, ouvrez le port **pour le réseau local uniquement**, par exemple :
   ```bash
   sudo ufw allow from 192.168.1.0/24 to any port 8737 proto tcp
   ```
5. *(Recommandé, pour les mises à jour automatiques)* **Réglages → Mise à jour des bornes → Créer la clé de signature**, puis
   **Avancé → Clé publique → Copier**.

### 2.3 Préparer la carte SD

Avec [Raspberry Pi Imager](https://www.raspberrypi.com/software/) :

1. Système : **Raspberry Pi OS Lite (64-bit)** (sans bureau : la borne gère elle-même l'écran).
2. Dans les réglages avancés (roue dentée / « Modifier les réglages ») :
   - nom d'hôte (ex. `borne1`), utilisateur et mot de passe ;
   - Wi-Fi (SSID, mot de passe, pays `FR`) si pas de câble ;
   - fuseau horaire **`Europe/Paris`** ;
   - **activer SSH**.
3. Écrivez la carte, insérez-la dans le Pi, branchez lecteur, écran et alimentation.

> Le Pi n'a pas de pile pour son horloge : ce n'est pas grave, l'appli corrige l'heure des badges d'après l'écart mesuré avec la borne.

### 2.4 Installer le paquet

Récupérez `badgeuse-kiosk_X.Y.Z_arm64.deb` dans la même [release](https://github.com/epitech-toulouse/IDcard_scanner/releases/latest)
que l'appli du PC, copiez-le sur le Pi puis installez-le :

```bash
# depuis le PC
scp badgeuse-kiosk_*_arm64.deb <utilisateur>@borne1.local:

# sur le Pi (ssh <utilisateur>@borne1.local)
sudo apt update
sudo apt install ./badgeuse-kiosk_*_arm64.deb
```

> **Toujours `apt install ./…`**, pas `dpkg -i` : apt installe les dépendances (`pcscd`, `libccid`, `cage`, `chromium`, `avahi-daemon`…).

Le paquet s'occupe de tout le reste :

- crée les utilisateurs système `badgeuse-kiosk` (lecture NFC) et `badgeuse-display` (écran) ;
- bloque le pilote `pn533_usb` (`/etc/modprobe.d/blacklist-acr122u.conf`) ;
- désactive le bureau éventuel (lightdm, gdm…) et démarre en mode console : **l'écran appartient à la borne** ;
- installe un curseur invisible et une politique Chromium sans synchro, traduction ni rapports ;
- active la surveillance des mises à jour (`badgeuse-kiosk-update.path`).

### 2.5 Configurer la borne

```bash
sudo nano /etc/badgeuse-kiosk.toml
```

Les champs à renseigner :

```toml
server_url = "http://192.168.1.20:8737"   # adresse notée à l'étape 2.2 (ou http://nom-du-pc.local:8737)
token = "…"                               # jeton copié depuis l'appli
device = "borne1"                         # nom unique par borne (lettres, chiffres, - et _)
update_public_key = "…"                   # clé publique de l'étape 2.2 (vide = pas de mise à jour automatique)
```

Réglages facultatifs du même fichier : `reader` (`auto`, `pcsc`, `mock`), `feedback` (LED/bip), `beep` (`normal`, `discreet`,
`silent`), `show_secs` (durée d'affichage du résultat, 1 à 30 s).

### 2.6 Démarrer

```bash
sudo systemctl enable --now badgeuse-kiosk badgeuse-display
sudo reboot      # recommandé à la première installation : applique la blacklist et passe l'écran sur la borne
```

Au redémarrage, l'écran affiche la page d'attente de la borne en plein écran. Dans l'appli du PC, la borne apparaît dans le
statut des lecteurs sous le nom `borne:borne1` (hors ligne après 30 s sans signal).

**Test** : ouvrez un créneau dans l'appli, badgez sur la borne : le nom doit s'afficher sur la borne *et* dans l'écran
d'émargement du PC.

### 2.7 Commandes utiles sur la borne

```bash
systemctl status badgeuse-kiosk badgeuse-display   # état des services
journalctl -u badgeuse-kiosk -f                    # journaux de la lecture / de l'envoi
journalctl -u badgeuse-display -f                  # journaux de l'affichage
sudo systemctl restart badgeuse-kiosk              # après une modification de /etc/badgeuse-kiosk.toml
pcsc_scan                                          # le lecteur est-il vu ? (paquet pcsc-tools)
```

**Désinstaller** : `sudo apt remove badgeuse-kiosk` rétablit le bureau d'origine au prochain démarrage.
`sudo apt purge badgeuse-kiosk` supprime aussi la configuration, les utilisateurs système et les badges non transmis.

### 2.8 Essayer sans Pi ni lecteur

Sur un PC, avec une configuration contenant `reader = "mock"` :

```bash
cargo run -p badgeuse-kiosk -- --config ma-conf.toml
```

puis ouvrez `http://127.0.0.1:8080` : un champ **Badger** en bas à gauche simule un badge.

---

## 3. Utilisation

1. **Étudiants → Importer** : au besoin, **Télécharger le modèle (CSV)** (ou `fixtures/modele_import_etudiants.csv`) pour les
   colonnes attendues ; choisissez le fichier, vérifiez les colonnes, puis choisissez ou créez la promotion.
2. **Créneaux → Nouveau créneau** : intitulé, horaires, seuil de retard, puis groupes ou promotions attendus.
3. **Émarger → Ouvrir l'émargement**. Les étudiants badgent (sur le lecteur du PC ou sur une borne). Une carte inconnue vous
   demande à quel étudiant l'associer, une seule fois.
4. **Clore l'émargement**, puis **Export Excel**.

Pratique :

- **Réglages → Créneaux → Émargement automatique** : ouvre les créneaux prévus quelques minutes avant leur début, et peut les
  clore à leur fin.
- **Affichage** : plein écran à projeter (créneau en cours, puis « prochain émargement à … »).
- **Cartes** : page de test qui identifie une carte (type, UID, ATR) sans rien enregistrer.
- **Réglages → Sauvegarder la base…** : à faire régulièrement (copie du fichier SQLite).

Statuts : **présent** si le badge arrive au plus tard à *début + seuil de retard*, sinon **en retard**. À la clôture, les attendus
sans badge passent **absents** (annulé si l'on rouvre le créneau). Un badge tardif ne rétrograde jamais un présent.

---

## 4. Mises à jour

### Appli du PC

L'appli vérifie au démarrage, puis chaque jour, si une nouvelle version est publiée et propose de l'installer (paquet signé,
vérifié avant installation, puis redémarrage). Désactivable dans **Réglages → Mise à jour de Badgeuse**.

### Bornes

L'appli embarque le paquet de la borne de sa propre version. Après une mise à jour de l'appli :

1. **Réglages → Mise à jour des bornes → Mettre à jour les bornes**.
2. Chaque borne télécharge le paquet dès qu'elle est au repos (aucun badge depuis 60 s, file d'attente vide), vérifie sa
   signature et l'installe seule.

Conditions : la clé publique de l'appli doit être dans `update_public_key` de chaque borne (étape 2.5). Une borne n'installe
jamais une version plus ancienne, ni un paquet non signé par la clé de l'appli. Si vous régénérez la clé, recopiez-la sur
toutes les bornes.

Mise à jour manuelle d'une borne : recopiez le nouveau `.deb` et relancez `sudo apt install ./badgeuse-kiosk_*_arm64.deb`
(la configuration est conservée).

---

## 5. Dépannage

| Symptôme | Piste |
|---|---|
| Lecteur non détecté sur le PC | Blacklist `pn533_usb` faite ? Lecteur débranché/rebranché ? `pcsc_scan` voit-il le lecteur ? Sinon **Réglages → Lecteur → USB direct ACR122U**. Diagnostic sans pcscd : `python3 tools/acr122u_test.py 30`. |
| Une carte n'est pas reconnue | Page **Cartes** de l'appli, ou `python3 tools/carte_tool.py`. Les cartes à UID aléatoire (téléphones, certaines cartes bancaires) sont refusées. |
| La borne n'apparaît pas dans l'appli | Bornes activées **et appli relancée** ? Même réseau ? `curl http://<ip-du-pc>:8737` depuis le Pi ; pare-feu du PC ; jeton identique ; `journalctl -u badgeuse-kiosk -f`. |
| La borne affiche une erreur de jeton | Le jeton a été régénéré dans l'appli : recopiez-le dans `/etc/badgeuse-kiosk.toml` puis `sudo systemctl restart badgeuse-kiosk`. |
| Écran de la borne noir ou console | `systemctl status badgeuse-display`, puis `sudo reboot` (nécessaire si un bureau tournait pendant l'installation). |
| Les bornes ne se mettent pas à jour | `update_public_key` vide ou différente de la clé de l'appli ; borne jamais au repos ; `journalctl -u badgeuse-kiosk-update`. |
| Réseau coupé pendant un cours | Rien à faire : la borne garde les badges et les renvoie (toutes les 10 s) avec leur heure, même après la clôture du créneau. |

---

## 6. Développement

```bash
sudo apt install libwebkit2gtk-4.1-dev libpcsclite-dev pcscd libccid pcsc-tools
npm install
npm run tauri dev                         # appli, vrai lecteur
BADGEUSE_READER=mock npm run tauri dev    # lecteur simulé (champ « Badger » sur l'écran d'émargement)
```

Tests et vérifications :

```bash
cargo test -p badgeuse-core      # logique métier
cargo test -p badgeuse-kiosk     # borne
npm run check                    # svelte-check
npm test                         # vitest
```

Paquets :

```bash
npm run tauri build              # .deb + AppImage dans target/release/bundle/
cargo deb -p badgeuse-kiosk      # .deb de la borne (à compiler sur un Pi ou une machine arm64)
```

**Publication** : chaque push sur `main` fait calculer la version par la CI (`[feat]` → mineure, `[breaking]` → majeure,
sinon correctif), compiler la borne puis l'appli, et publier la release GitHub. `[skip release]` dans le message de commit :
rien n'est publié. Les détails d'architecture sont dans [`CLAUDE.md`](CLAUDE.md).

---

## 7. Données personnelles et sécurité

- Toutes les données restent sur le PC de l'enseignant : aucun service externe, aucune télémétrie, aucun CDN.
  Seule exception : la recherche de mise à jour de l'appli sur GitHub (aucune donnée envoyée, désactivable).
- Les bornes ne reçoivent que **le prénom et l'initiale du nom** et les totaux du créneau ouvert ; leur file d'attente ne
  contient que l'UID et l'heure du badge, et se vide dès la transmission.
- Le trafic borne → PC est en **HTTP non chiffré** : le jeton protège l'accès mais circule en clair. Réservez-le à un réseau
  local de confiance et **n'exposez jamais le port vers Internet**.
- Le jeton seul ne permet pas d'installer un paquet sur une borne : il faut la signature de la clé privée, qui ne quitte pas
  l'appli.
- Fin d'année : supprimer la promotion efface ses étudiants et leurs présences.
