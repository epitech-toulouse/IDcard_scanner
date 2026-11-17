//! Règles métier pures (sans base de données).

use crate::model::Status;
use chrono::{Duration, NaiveDateTime};
use serde::Serialize;

/// Statut d'un badge : présent jusqu'à `début + seuil` inclus, en retard ensuite.
pub fn status_for_scan(starts_at: NaiveDateTime, late_after_min: i64, scanned_at: NaiveDateTime) -> Status {
    if scanned_at <= starts_at + Duration::minutes(late_after_min) {
        Status::Present
    } else {
        Status::Late
    }
}

/// Un nouveau badge peut-il remplacer le statut existant ?
/// On ne rétrograde jamais un présent/retard, mais un badge corrige une absence.
pub fn scan_overrides(existing: Option<Status>) -> bool {
    !matches!(existing, Some(Status::Present | Status::Late))
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Tally {
    pub sessions: u32,
    pub present: u32,
    pub late: u32,
    pub absent: u32,
    pub excused: u32,
}

impl Tally {
    pub fn add(&mut self, s: Option<Status>) {
        self.sessions += 1;
        match s {
            Some(Status::Present) => self.present += 1,
            Some(Status::Late) => self.late += 1,
            Some(Status::Excused) => self.excused += 1,
            // Non renseigné (créneau non clos) compté comme absent.
            Some(Status::Absent) | None => self.absent += 1,
        }
    }
    pub fn merge(&mut self, o: &Tally) {
        self.sessions += o.sessions;
        self.present += o.present;
        self.late += o.late;
        self.absent += o.absent;
        self.excused += o.excused;
    }
    /// Taux de présence (présents + retards) / créneaux, entre 0 et 1.
    pub fn attendance_rate(&self) -> f64 {
        if self.sessions == 0 {
            return 0.0;
        }
        (self.present + self.late) as f64 / self.sessions as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn present_until_threshold_inclusive() {
        let start = dt("2026-11-17 08:00");
        assert_eq!(status_for_scan(start, 10, dt("2026-11-17 07:50")), Status::Present);
        assert_eq!(status_for_scan(start, 10, dt("2026-11-17 08:10")), Status::Present);
        assert_eq!(status_for_scan(start, 10, dt("2026-11-17 08:11")), Status::Late);
        assert_eq!(status_for_scan(start, 0, dt("2026-11-17 08:01")), Status::Late);
    }

    #[test]
    fn overrides() {
        assert!(scan_overrides(None));
        assert!(scan_overrides(Some(Status::Absent)));
        assert!(scan_overrides(Some(Status::Excused)));
        assert!(!scan_overrides(Some(Status::Present)));
        assert!(!scan_overrides(Some(Status::Late)));
    }

    #[test]
    fn tally_rate() {
        let mut t = Tally::default();
        for s in [Some(Status::Present), Some(Status::Late), Some(Status::Absent), None] {
            t.add(s);
        }
        assert_eq!(t.absent, 2);
        assert!((t.attendance_rate() - 0.5).abs() < 1e-9);
        assert_eq!(Tally::default().attendance_rate(), 0.0);
    }
}
