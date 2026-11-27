//! Règles métier pures (sans base de données).

use crate::model::{Recurrence, SeriesPlan, SessionState, Status};
use crate::{Error, Result};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
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

/// Un créneau accueille-t-il un badge pris à l'instant `at` ?
/// De `début − avance` jusqu'à la fin (exclue), comme l'ouverture automatique.
pub fn session_window_contains(starts_at: NaiveDateTime, ends_at: NaiveDateTime, lead_min: i64, at: NaiveDateTime) -> bool {
    at >= starts_at - Duration::minutes(lead_min.max(0)) && at < ends_at
}

/// Créneau qui reçoit un badge rejoué par une borne. `candidates` : (id, début) des créneaux
/// dont la fenêtre contient l'heure du badge. On garde l'actif s'il en fait partie, sinon le
/// plus tôt commencé ; à défaut, l'actif (même traitement qu'un badge en direct).
pub fn pick_remote_session(active: Option<i64>, candidates: &[(i64, NaiveDateTime)]) -> Option<i64> {
    if let Some(a) = active.filter(|a| candidates.iter().any(|c| c.0 == *a)) {
        return Some(a);
    }
    candidates.iter().min_by_key(|c| (c.1, c.0)).map(|c| c.0).or(active)
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

// ---------- Répétition des créneaux ----------

/// Garde-fou : une série ne crée jamais plus de créneaux que cela.
pub const MAX_SERIES: usize = 200;
/// Une série ne s'étend pas au-delà d'un an après sa première date.
const MAX_SERIES_DAYS: i64 = 366;

/// Dimanche de Pâques (calendrier grégorien, algorithme de Meeus/Jones/Butcher).
pub fn easter(year: i32) -> NaiveDate {
    let a = year % 19;
    let (b, c) = (year / 100, year % 100);
    let (d, e) = (b / 4, b % 4);
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let (i, k) = (c / 4, c % 4);
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32).expect("date de Pâques valide")
}

/// Jour férié en France métropolitaine (fêtes fixes, lundi de Pâques, Ascension, lundi de Pentecôte).
pub fn is_french_holiday(d: NaiveDate) -> bool {
    const FIXED: [(u32, u32); 8] = [(1, 1), (5, 1), (5, 8), (7, 14), (8, 15), (11, 1), (11, 11), (12, 25)];
    if FIXED.contains(&(d.month(), d.day())) {
        return true;
    }
    let e = easter(d.year());
    [1, 39, 50].iter().any(|n| e + Duration::days(*n) == d)
}

/// Dates d'une série à partir de `first` (incluse si elle correspond à la règle).
/// Les semaines se comptent depuis le lundi de la semaine de `first` : avec `interval_weeks = 2`,
/// on garde la semaine de `first`, puis une sur deux.
pub fn recurrence_plan(first: NaiveDate, rule: &Recurrence) -> Result<SeriesPlan> {
    if rule.weekdays.is_empty() || rule.weekdays.iter().any(|w| !(1..=7).contains(w)) {
        return Err(Error::Invalid("Choisissez au moins un jour de la semaine".into()));
    }
    if !(1..=4).contains(&rule.interval_weeks) {
        return Err(Error::Invalid("L'intervalle doit être de 1 à 4 semaines".into()));
    }
    let limit = first + Duration::days(MAX_SERIES_DAYS);
    let until = match (rule.until, rule.count) {
        (Some(u), _) if u < first => return Err(Error::Invalid("La date de fin est avant le premier créneau".into())),
        (Some(u), _) => u.min(limit),
        (None, Some(n)) if n >= 1 => limit,
        _ => return Err(Error::Invalid("Indiquez une date de fin ou un nombre de créneaux".into())),
    };
    let max = rule.count.map_or(MAX_SERIES, |n| (n as usize).min(MAX_SERIES));
    let week0 = first - Duration::days(first.weekday().num_days_from_monday() as i64);
    let mut plan = SeriesPlan::default();
    let mut d = first;
    while d <= until && plan.dates.len() < max {
        let week = (d - week0).num_days() / 7;
        let weekday = d.weekday().number_from_monday() as u8;
        if week % rule.interval_weeks as i64 == 0 && rule.weekdays.contains(&weekday) {
            if rule.skip_holidays && is_french_holiday(d) {
                plan.skipped.push(d);
            } else {
                plan.dates.push(d);
            }
        }
        d += Duration::days(1);
    }
    if plan.dates.is_empty() {
        return Err(Error::Invalid("Aucune date ne correspond à cette répétition".into()));
    }
    Ok(plan)
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
    fn session_window() {
        let (s, e) = (dt("2026-11-17 08:00"), dt("2026-11-17 10:00"));
        assert!(!session_window_contains(s, e, 10, dt("2026-11-17 07:49")));
        assert!(session_window_contains(s, e, 10, dt("2026-11-17 07:50")));
        assert!(session_window_contains(s, e, 10, dt("2026-11-17 09:59")));
        assert!(!session_window_contains(s, e, 10, dt("2026-11-17 10:00")));
        // Avance négative ignorée.
        assert!(!session_window_contains(s, e, -5, dt("2026-11-17 07:59")));
    }

    #[test]
    fn remote_session_choice() {
        let (a, b) = ((1, dt("2026-11-17 08:00")), (2, dt("2026-11-17 08:30")));
        // L'actif gagne s'il est candidat, même s'il commence plus tard.
        assert_eq!(pick_remote_session(Some(2), &[a, b]), Some(2));
        // Sinon le plus tôt commencé.
        assert_eq!(pick_remote_session(Some(9), &[b, a]), Some(1));
        assert_eq!(pick_remote_session(None, &[b, a]), Some(1));
        // Aucun candidat : l'actif tel quel, ou rien.
        assert_eq!(pick_remote_session(Some(9), &[]), Some(9));
        assert_eq!(pick_remote_session(None, &[]), None);
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

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn rule(weekdays: &[u8], interval: u32, until: Option<&str>, count: Option<u32>, skip: bool) -> Recurrence {
        Recurrence { weekdays: weekdays.to_vec(), interval_weeks: interval, until: until.map(d), count, skip_holidays: skip }
    }

    #[test]
    fn easter_dates() {
        assert_eq!(easter(2026), d("2026-04-05"));
        assert_eq!(easter(2027), d("2027-03-28"));
        assert_eq!(easter(2024), d("2024-03-31"));
    }

    #[test]
    fn french_holidays() {
        assert!(is_french_holiday(d("2026-12-25")));
        assert!(is_french_holiday(d("2027-01-01")));
        assert!(is_french_holiday(d("2027-03-29"))); // lundi de Pâques
        assert!(is_french_holiday(d("2027-05-06"))); // Ascension
        assert!(is_french_holiday(d("2027-05-17"))); // lundi de Pentecôte
        assert!(!is_french_holiday(d("2026-12-24")));
    }

    #[test]
    fn weekdays_until_date() {
        // Du lundi au vendredi, du lun. 30 nov. au ven. 18 déc. 2026 : 15 créneaux.
        let p = recurrence_plan(d("2026-11-30"), &rule(&[1, 2, 3, 4, 5], 1, Some("2026-12-18"), None, false)).unwrap();
        assert_eq!(p.dates.len(), 15);
        assert_eq!(p.dates[0], d("2026-11-30"));
        assert_eq!(*p.dates.last().unwrap(), d("2026-12-18"));
        assert!(p.dates.iter().all(|x| x.weekday().number_from_monday() <= 5));
    }

    #[test]
    fn first_date_not_matching_is_skipped() {
        // Début un mercredi, répétition le lundi seulement : premier créneau le lundi suivant.
        let p = recurrence_plan(d("2026-12-02"), &rule(&[1], 1, None, Some(2), false)).unwrap();
        assert_eq!(p.dates, vec![d("2026-12-07"), d("2026-12-14")]);
    }

    #[test]
    fn every_other_week() {
        // Semaine du premier créneau, puis une sur deux (même si on commence un jeudi).
        let p = recurrence_plan(d("2026-12-03"), &rule(&[1, 4], 2, Some("2026-12-31"), None, false)).unwrap();
        assert_eq!(p.dates, vec![d("2026-12-03"), d("2026-12-14"), d("2026-12-17"), d("2026-12-28"), d("2026-12-31")]);
    }

    #[test]
    fn count_and_holidays() {
        // Vendredis à partir du 18 déc. : le 25 déc. et le 1er janv. sont fériés.
        let p = recurrence_plan(d("2026-12-18"), &rule(&[5], 1, None, Some(3), true)).unwrap();
        assert_eq!(p.dates, vec![d("2026-12-18"), d("2027-01-08"), d("2027-01-15")]);
        assert_eq!(p.skipped, vec![d("2026-12-25"), d("2027-01-01")]);
        // Sans l'option, les fériés sont gardés.
        let p = recurrence_plan(d("2026-12-18"), &rule(&[5], 1, None, Some(3), false)).unwrap();
        assert_eq!(p.dates[1], d("2026-12-25"));
    }

    #[test]
    fn invalid_rules() {
        let first = d("2026-11-30");
        assert!(recurrence_plan(first, &rule(&[], 1, Some("2026-12-18"), None, false)).is_err());
        assert!(recurrence_plan(first, &rule(&[8], 1, Some("2026-12-18"), None, false)).is_err());
        assert!(recurrence_plan(first, &rule(&[1], 0, Some("2026-12-18"), None, false)).is_err());
        assert!(recurrence_plan(first, &rule(&[1], 1, None, None, false)).is_err());
        assert!(recurrence_plan(first, &rule(&[1], 1, Some("2026-11-01"), None, false)).is_err());
        // Un seul mardi demandé sur une plage sans mardi.
        assert!(recurrence_plan(first, &rule(&[2], 1, Some("2026-11-30"), None, false)).is_err());
    }

    #[test]
    fn capped() {
        let p = recurrence_plan(d("2026-01-01"), &rule(&[1, 2, 3, 4, 5, 6, 7], 1, None, Some(5000), false)).unwrap();
        assert_eq!(p.dates.len(), MAX_SERIES);
        let p = recurrence_plan(d("2026-01-01"), &rule(&[1], 1, Some("2030-01-01"), None, false)).unwrap();
        assert!(*p.dates.last().unwrap() <= d("2027-01-02"));
    }
}
