//! Règles métier pures (sans base de données).

use crate::model::{SessionState, Status};
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

/// Transition automatique d'un créneau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoAction {
    Open,
    Close,
}

/// Ouverture auto (`open_lead_min` = avance, `None` = désactivée) : créneau prévu, de `début − avance` jusqu'à la fin (exclue) ; jamais après coup.
/// Clôture auto (si activée) : créneau ouvert dont la fin est passée.
pub fn auto_action(
    state: SessionState,
    starts_at: NaiveDateTime,
    ends_at: NaiveDateTime,
    now: NaiveDateTime,
    open_lead_min: Option<i64>,
    auto_close: bool,
) -> Option<AutoAction> {
    match state {
        SessionState::Planned
            if open_lead_min.is_some_and(|l| now >= starts_at - Duration::minutes(l.max(0))) && now < ends_at =>
        {
            Some(AutoAction::Open)
        }
        SessionState::Open if auto_close && now >= ends_at => Some(AutoAction::Close),
        _ => None,
    }
}

/// Créneau actif (celui qui reçoit les badges), réévalué en continu :
/// - l'actif est gardé tant qu'il est ouvert et pas terminé ;
/// - sinon, le créneau ouvert en cours (fin non passée) qui commence le plus tôt le remplace ;
/// - à défaut, un actif ouvert mais terminé est gardé (retardataires, correction du prof).
/// `current` et `open` : (id, état, début, fin).
pub fn pick_active(
    current: Option<(i64, SessionState, NaiveDateTime, NaiveDateTime)>,
    open: &[(i64, SessionState, NaiveDateTime, NaiveDateTime)],
    now: NaiveDateTime,
) -> Option<i64> {
    let running = |s: &(i64, SessionState, NaiveDateTime, NaiveDateTime)| s.1 == SessionState::Open && now < s.3;
    if let Some(c) = current.filter(running) {
        return Some(c.0);
    }
    if let Some(s) = open.iter().filter(|s| running(s)).min_by_key(|s| (s.2, s.0)) {
        return Some(s.0);
    }
    current.filter(|c| c.1 == SessionState::Open).map(|c| c.0)
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
    fn auto_open_close_window() {
        use SessionState::*;
        let (s, e) = (dt("2026-11-17 08:00"), dt("2026-11-17 10:00"));
        assert_eq!(auto_action(Planned, s, e, dt("2026-11-17 07:49"), Some(10), false), None);
        assert_eq!(auto_action(Planned, s, e, dt("2026-11-17 07:50"), Some(10), false), Some(AutoAction::Open));
        assert_eq!(auto_action(Planned, s, e, dt("2026-11-17 09:59"), Some(10), false), Some(AutoAction::Open));
        // Créneau déjà terminé : pas d'ouverture rétroactive.
        assert_eq!(auto_action(Planned, s, e, dt("2026-11-17 10:00"), Some(10), true), None);
        assert_eq!(auto_action(Open, s, e, dt("2026-11-17 10:00"), Some(10), false), None);
        assert_eq!(auto_action(Open, s, e, dt("2026-11-17 10:00"), Some(10), true), Some(AutoAction::Close));
        assert_eq!(auto_action(Open, s, e, dt("2026-11-17 09:00"), Some(10), true), None);
        assert_eq!(auto_action(Planned, s, e, dt("2026-11-17 08:30"), None, true), None);
        assert_eq!(auto_action(Closed, s, e, dt("2026-11-17 09:00"), Some(10), true), None);
    }

    #[test]
    fn active_follows_running_session() {
        use SessionState::*;
        let a = (1, Open, dt("2026-11-17 16:25"), dt("2026-11-17 16:28"));
        let b = (2, Open, dt("2026-11-17 16:30"), dt("2026-11-17 16:35"));
        let open = [a, b];
        // L'actif en cours est gardé, même si un autre a été ouvert en avance.
        assert_eq!(pick_active(Some(a), &open, dt("2026-11-17 16:26")), Some(1));
        // Une fois terminé, le créneau ouvert suivant prend la main.
        assert_eq!(pick_active(Some(a), &open, dt("2026-11-17 16:29")), Some(2));
        assert_eq!(pick_active(None, &open, dt("2026-11-17 16:29")), Some(2));
        // Plus rien en cours : on garde l'actif ouvert (retardataires), pas un clos.
        assert_eq!(pick_active(Some(b), &open, dt("2026-11-17 16:40")), Some(2));
        assert_eq!(pick_active(Some((3, Closed, a.2, a.3)), &[], dt("2026-11-17 16:40")), None);
        assert_eq!(pick_active(None, &open, dt("2026-11-17 16:40")), None);
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
