#!/bin/sh
# Prochaine version, d'après le dernier tag vX.Y.Z et les messages de commit depuis ce tag :
#   « [breaking] » ou « BREAKING CHANGE » → majeure (en 0.x : mineure) ; « [feat] » → mineure ; sinon → correctif.
# Sans tag : 0.1.0. Affiche la version sans le « v ».
set -eu
last=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)
if [ -z "$last" ]; then
  echo "0.1.0"
  exit 0
fi
msgs=$(git log "$last..HEAD" --format=%B)
v=${last#v}
major=${v%%.*}; rest=${v#*.}; minor=${rest%%.*}; patch=${rest#*.}
if printf '%s' "$msgs" | grep -qiE '^\[breaking\]|BREAKING CHANGE'; then
  if [ "$major" -eq 0 ]; then minor=$((minor + 1)); patch=0; else major=$((major + 1)); minor=0; patch=0; fi
elif printf '%s' "$msgs" | grep -qiE '^\[feat\]'; then
  minor=$((minor + 1)); patch=0
else
  patch=$((patch + 1))
fi
echo "$major.$minor.$patch"
