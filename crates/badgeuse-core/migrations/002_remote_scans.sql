-- Badges reçus d'une borne réseau : idempotence du rejeu (même scan_id = même réponse) et audit.
-- Purgés automatiquement au bout de 30 jours (voir remote::PURGE_DAYS).
CREATE TABLE remote_scans (
    scan_id     TEXT PRIMARY KEY,   -- "<borne>-<n°>", fixé par la borne
    device      TEXT NOT NULL,
    uid         TEXT NOT NULL,
    scanned_at  TEXT NOT NULL,      -- heure du badge, corrigée de l'écart d'horloge
    received_at TEXT NOT NULL,
    reply_json  TEXT NOT NULL       -- RemoteReply renvoyée à la borne
);
CREATE INDEX remote_scans_received ON remote_scans(received_at);
