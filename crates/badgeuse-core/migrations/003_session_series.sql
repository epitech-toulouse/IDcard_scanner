-- Créneaux répétés : une série regroupe des créneaux ordinaires (un par date), créés d'un coup.
-- Chaque créneau garde son propre état et ses présences ; la série sert à modifier ou supprimer ensemble.
CREATE TABLE session_series (
    id         INTEGER PRIMARY KEY,
    rule_json  TEXT NOT NULL,   -- model::Recurrence, pour mémoire
    created_at TEXT NOT NULL
);
ALTER TABLE sessions ADD COLUMN series_id INTEGER REFERENCES session_series(id) ON DELETE SET NULL;
CREATE INDEX sessions_series ON sessions(series_id);
