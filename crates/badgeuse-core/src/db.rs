use crate::Result;
use chrono::{NaiveDateTime, Timelike};
pub use rusqlite::Connection;
use std::path::Path;

/// Migrations appliquées dans l'ordre ; l'index+1 est stocké dans `PRAGMA user_version`.
/// Ne jamais modifier une migration existante : en ajouter une nouvelle à la fin.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_init.sql"),
    include_str!("../migrations/002_remote_scans.sql"),
];

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    init(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    init(Connection::open_in_memory()?)
}

fn init(mut conn: Connection) -> Result<Connection> {
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&mut conn)?;
    Ok(conn)
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
        tracing::info!("migration {} appliquée", i + 1);
    }
    Ok(())
}

/// Heure locale actuelle, à la seconde.
pub fn now() -> NaiveDateTime {
    chrono::Local::now().naive_local().with_nanosecond(0).unwrap()
}

/// Copie cohérente de la base vers `dest` (fonctionne base ouverte).
pub fn backup_to(conn: &Connection, dest: &Path) -> Result<()> {
    conn.backup(rusqlite::MAIN_DB, dest, None::<fn(rusqlite::backup::Progress)>)?;
    Ok(())
}

/// Remplace le contenu de la base par celui de `src` (puis ré-applique les migrations).
pub fn restore_from(conn: &mut Connection, src: &Path) -> Result<()> {
    conn.restore(rusqlite::MAIN_DB, src, None::<fn(rusqlite::backup::Progress)>)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(conn)
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrations_idempotent() {
        let mut c = super::open_in_memory().unwrap();
        super::migrate(&mut c).unwrap();
        let v: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(v as usize, super::MIGRATIONS.len());
    }
}
