-- Schéma initial. NE PAS MODIFIER une fois publié : ajouter 002_xxx.sql à la place.
-- Vocabulaire : program = cursus, cohort = promotion, group = groupe TD/TP,
-- session = créneau d'émargement, attendance = présence.

CREATE TABLE programs (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE cohorts (
    id          INTEGER PRIMARY KEY,
    program_id  INTEGER NOT NULL REFERENCES programs(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    school_year TEXT NOT NULL,
    UNIQUE (program_id, name, school_year)
);

CREATE TABLE groups (
    id        INTEGER PRIMARY KEY,
    cohort_id INTEGER NOT NULL REFERENCES cohorts(id) ON DELETE CASCADE,
    name      TEXT NOT NULL,
    UNIQUE (cohort_id, name)
);

CREATE TABLE students (
    id             INTEGER PRIMARY KEY,
    student_number TEXT NOT NULL UNIQUE,
    last_name      TEXT NOT NULL,
    first_name     TEXT NOT NULL,
    email          TEXT,
    cohort_id      INTEGER REFERENCES cohorts(id) ON DELETE SET NULL,
    active         INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE student_groups (
    student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE CASCADE,
    group_id   INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    PRIMARY KEY (student_id, group_id)
);

CREATE TABLE cards (
    uid        TEXT PRIMARY KEY,           -- "04:24:A5:82:E6:19:90"
    student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE CASCADE,
    linked_at  TEXT NOT NULL
);
CREATE INDEX cards_student ON cards(student_id);

CREATE TABLE sessions (
    id             INTEGER PRIMARY KEY,
    title          TEXT NOT NULL,
    room           TEXT,
    starts_at      TEXT NOT NULL,
    ends_at        TEXT NOT NULL,
    late_after_min INTEGER NOT NULL DEFAULT 10,
    state          TEXT NOT NULL DEFAULT 'planned' CHECK (state IN ('planned', 'open', 'closed'))
);
CREATE INDEX sessions_start ON sessions(starts_at);

-- Étudiants attendus = membres des groupes ciblés ∪ membres des promotions ciblées.
CREATE TABLE session_groups (
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    group_id   INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    PRIMARY KEY (session_id, group_id)
);
CREATE TABLE session_cohorts (
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    cohort_id  INTEGER NOT NULL REFERENCES cohorts(id) ON DELETE CASCADE,
    PRIMARY KEY (session_id, cohort_id)
);

CREATE TABLE attendance (
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE CASCADE,
    status     TEXT NOT NULL CHECK (status IN ('present', 'late', 'absent', 'excused')),
    scanned_at TEXT,
    source     TEXT NOT NULL CHECK (source IN ('badge', 'manual', 'auto')),
    reason     TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (session_id, student_id)
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
