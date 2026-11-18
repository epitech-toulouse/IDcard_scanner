# Badgeuse

Émargement des étudiants par carte NFC, pour Linux.

- **Créneaux d'émargement** : ouverture, badge, retard automatique au-delà d'un seuil, corrections manuelles, absences justifiées, clôture (les non-badgés passent absents).
- **Écran live** et **mode projection** : le nom s'affiche en vert, orange ou rouge à chaque badge.
- **Groupes** : cursus → promotion → groupes TD/TP.
- **Import** de la liste des étudiants depuis un CSV ou un fichier Excel de la scolarité. Chaque carte est associée à son étudiant au premier badge.
- **Exports Excel** : feuille d'un créneau, ou récapitulatif sur une période (matrice étudiants × créneaux et statistiques par étudiant et par groupe).
- Données 100 % locales (SQLite), sauvegarde et restauration en un clic.

Lecteur testé : ACS ACR122U, par PC/SC ou en accès USB direct sans pcscd.

## Installation (développement)

```bash
sudo apt install libwebkit2gtk-4.1-dev libpcsclite-dev pcscd libccid pcsc-tools
echo "blacklist pn533_usb" | sudo tee /etc/modprobe.d/blacklist-acr122u.conf
sudo modprobe -r pn533_usb   # ou débrancher / rebrancher le lecteur

npm install
npm run tauri dev
```

Sans lecteur : `BADGEUSE_READER=mock npm run tauri dev`, puis importez `fixtures/etudiants.csv`.

## Paquet

`npm run tauri build` produit un `.deb` et un AppImage dans `target/release/bundle/`.

## Borne de scan (Raspberry Pi)

Pour badger à l'entrée de la salle : un Raspberry Pi 4 avec l'ACR122U et un écran HDMI envoie les badges à l'appli du PC de
l'enseignant, par le **réseau local**. L'appli reste le tableau de bord (créneaux, présences, exports). Si le réseau est coupé,
la borne garde les badges et les renvoie au retour, avec leur heure d'origine.

**Côté PC** : Réglages → **Bornes réseau** → cocher « Accepter les bornes réseau », enregistrer, relancer l'appli. Notez l'adresse
et le jeton affichés. Ouvrez le port (8737 par défaut) sur le réseau local uniquement.

**Côté Pi** (Raspberry Pi OS Lite 64 bits, installé avec Raspberry Pi Imager : SSH, réseau et fuseau `Europe/Paris` préconfigurés) :

```bash
sudo apt install pcscd libccid cage chromium avahi-daemon
sudo apt install ./badgeuse-kiosk_*_arm64.deb     # produit par `cargo deb -p badgeuse-kiosk` (ou par la CI)
sudo nano /etc/badgeuse-kiosk.toml                # server_url, token, device
sudo reboot                                       # recharge la blacklist pn533_usb
sudo systemctl enable --now badgeuse-kiosk badgeuse-display
```

Sans lecteur ni Pi, pour essayer sur un PC : `reader = "mock"` dans la configuration, puis `cargo run -p badgeuse-kiosk -- --config ma-conf.toml`
et ouvrir `http://127.0.0.1:8080` (champ « Badger » en bas à gauche). Journaux : `journalctl -u badgeuse-kiosk -f`.

Sécurité : le trafic est en HTTP, sans chiffrement ; le jeton protège l'accès mais circule en clair. À réserver à un réseau de confiance.

## Utilisation

1. **Étudiants → Importer** : au besoin, **Télécharger le modèle (CSV)** (ou `fixtures/modele_import_etudiants.csv`) pour les colonnes attendues, remplissez-le, choisissez le fichier, vérifiez les colonnes, puis choisissez ou créez la promotion.
2. **Créneaux → Nouveau créneau** : intitulé, horaires, seuil de retard, puis groupes ou promotion attendus.
3. **Émarger** → **Ouvrir l'émargement**. Les étudiants badgent. Une carte inconnue vous demande à quel étudiant l'associer.
4. **Clore l'émargement**, puis **Export Excel**.
