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

## Utilisation

1. **Étudiants → Importer** : choisissez le fichier, vérifiez les colonnes, puis choisissez ou créez la promotion.
2. **Créneaux → Nouveau créneau** : intitulé, horaires, seuil de retard, puis groupes ou promotion attendus.
3. **Émarger** → **Ouvrir l'émargement**. Les étudiants badgent. Une carte inconnue vous demande à quel étudiant l'associer.
4. **Clore l'émargement**, puis **Export Excel**.
