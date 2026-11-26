//! File d'attente locale (SQLite) des badges. Chaque badge y est écrit AVANT l'envoi : une coupure
//! de courant ou de réseau n'en perd aucun. Une ligne n'est supprimée qu'une fois traitée par l'appli.

use chrono::NaiveDateTime;
use rusqlite::{Connection, params};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub seq: i64,
    pub uid: String,
    pub scanned_at: NaiveDateTime,
}

pub struct Queue {
    conn: Connection,
    /// Instant de création de la base : évite qu'un `scan_id` se répète si le fichier est recréé.
    epoch: i64,
}

type Result<T> = std::result::Result<T, rusqlite::Error>;

impl Queue {
    pub fn open(path: &Path) -> std::result::Result<Queue, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("{} : {e}", path.display()))?;
        Queue::init(conn).map_err(|e| format!("file d'attente : {e}"))
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Queue {
        Queue::init(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn init(conn: Connection) -> Result<Queue> {
        // WAL + synchronous=FULL : un badge validé survit à une coupure de courant, avec un seul fsync
        // par écriture (au lieu de plusieurs en mode journal classique) : plus rapide et moins d'usure
        // de la carte SD. Sans effet sur une base en mémoire (tests).
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pending (
                 seq        INTEGER PRIMARY KEY AUTOINCREMENT,
                 uid        TEXT NOT NULL,
                 scanned_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('epoch', strftime('%s', 'now'))",
            [],
        )?;
        let epoch: i64 = conn.query_row("SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'epoch'", [], |r| r.get(0))?;
        Ok(Queue { conn, epoch })
    }

    /// Identifiant unique d'un badge, stable d'un renvoi à l'autre (idempotence côté appli).
    pub fn scan_id(&self, device: &str, seq: i64) -> String {
        format!("{device}-{}-{seq}", self.epoch)
    }

    pub fn push(&self, uid: &str, scanned_at: NaiveDateTime) -> Result<Pending> {
        self.conn.prepare_cached("INSERT INTO pending(uid, scanned_at) VALUES (?1, ?2)")?.execute(params![uid, scanned_at])?;
        Ok(Pending { seq: self.conn.last_insert_rowid(), uid: uid.into(), scanned_at })
    }

    /// Les `limit` badges les plus anciens, dans l'ordre d'arrivée.
    pub fn oldest(&self, limit: u32) -> Result<Vec<Pending>> {
        let mut st = self.conn.prepare_cached("SELECT seq, uid, scanned_at FROM pending ORDER BY seq LIMIT ?1")?;
        st.query_map([limit], |r| Ok(Pending { seq: r.get(0)?, uid: r.get(1)?, scanned_at: r.get(2)? }))?.collect()
    }

    pub fn remove(&self, seq: i64) -> Result<()> {
        self.conn.prepare_cached("DELETE FROM pending WHERE seq = ?1")?.execute([seq])?;
        Ok(())
    }

    pub fn len(&self) -> Result<u32> {
        self.conn.prepare_cached("SELECT COUNT(*) FROM pending")?.query_row([], |r| r.get(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn fifo_order_and_removal() {
        let q = Queue::open_in_memory();
        let a = q.push("04:A1", dt("2026-11-17 08:00:00")).unwrap();
        let b = q.push("04:B2", dt("2026-11-17 08:00:05")).unwrap();
        assert_eq!(q.len().unwrap(), 2);
        assert_eq!(q.oldest(10).unwrap(), vec![a.clone(), b.clone()]);
        assert_eq!(q.oldest(1).unwrap(), vec![a.clone()]);
        q.remove(a.seq).unwrap();
        assert_eq!(q.oldest(10).unwrap(), vec![b]);
        // Un numéro supprimé n'est jamais réattribué (le scan_id reste unique).
        let c = q.push("04:C3", dt("2026-11-17 08:01:00")).unwrap();
        assert!(c.seq > a.seq + 1);
    }

    #[test]
    fn survives_reopen_and_ids_are_stable() {
        let dir = std::env::temp_dir().join(format!("badgeuse-kiosk-test-{}", std::process::id()));
        let path = dir.join("queue.db");
        let first = {
            let q = Queue::open(&path).unwrap();
            q.push("04:A1", dt("2026-11-17 08:00:00")).unwrap();
            q.scan_id("borne1", 1)
        };
        let q = Queue::open(&path).unwrap();
        assert_eq!(q.len().unwrap(), 1, "le badge survit à un redémarrage");
        assert_eq!(q.scan_id("borne1", 1), first, "même identifiant après redémarrage");
        assert_ne!(q.scan_id("borne1", 2), first);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
