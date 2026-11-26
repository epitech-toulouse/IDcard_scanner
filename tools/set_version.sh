#!/bin/sh
# Inscrit une version (ex. 0.3.1) dans tous les fichiers qui la portent. Utilisé par la CI avant de compiler :
# la version n'est jamais commitée, elle vient du dernier tag git (voir .github/workflows/release.yml).
set -eu
v="${1:?usage : tools/set_version.sh X.Y.Z}"
case "$v" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "version invalide : $v" >&2; exit 2 ;;
esac
cd "$(dirname "$0")/.."
# Premier « version = » de chaque Cargo.toml (celui de [package]) et premier « "version": » des JSON.
for f in crates/badgeuse-core/Cargo.toml crates/badgeuse-kiosk/Cargo.toml src-tauri/Cargo.toml; do
  perl -0pi -e "s/^version = \".*?\"/version = \"$v\"/m" "$f"
  grep -q "^version = \"$v\"" "$f" || { echo "échec : $f" >&2; exit 1; }
done
for f in package.json src-tauri/tauri.conf.json; do
  perl -0pi -e "s/\"version\": \".*?\"/\"version\": \"$v\"/" "$f"
  grep -q "\"version\": \"$v\"" "$f" || { echo "échec : $f" >&2; exit 1; }
done
echo "version $v"
