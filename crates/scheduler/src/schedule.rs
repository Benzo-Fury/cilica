//! When a task runs next. Schedules are local wall-clock times; the maths is DST-safe:
//! a time that does not exist (spring forward) moves to the first valid minute after it, and a
//! time that happens twice (fall back) uses the first.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use margin_domain::TaskSchedule;

/// `date` at `hour:minute` in `tz`, as UTC.
pub fn local_time<Tz: TimeZone>(tz: &Tz, date: NaiveDate, hour: u8, minute: u8) -> DateTime<Utc> {
    let time = NaiveTime::from_hms_opt(u32::from(hour.min(23)), u32::from(minute.min(59)), 0).unwrap_or(NaiveTime::MIN);
    let naive = NaiveDateTime::new(date, time);
    let mut probe = naive;
    // A gap is at most a couple of hours; step forward a minute at a time until the time exists.
    for _ in 0..=(3 * 60) {
        match tz.from_local_datetime(&probe) {
            chrono::LocalResult::Single(t) => return t.with_timezone(&Utc),
            chrono::LocalResult::Ambiguous(first, _) => return first.with_timezone(&Utc),
            chrono::LocalResult::None => probe += Duration::minutes(1),
        }
    }
    // Unreachable for real zones; fall back to treating the time as UTC.
    Utc.from_utc_datetime(&naive)
}

/// The first run strictly after `after`. `last_run` anchors "every N days" schedules.
pub fn next_run<Tz: TimeZone>(schedule: &TaskSchedule, after: DateTime<Utc>, last_run: Option<DateTime<Utc>>, tz: &Tz) -> DateTime<Utc> {
    let today = after.with_timezone(tz).date_naive();
    let daily = |hour: u8, minute: u8| {
        (0..=2)
            .map(|d| local_time(tz, today + Duration::days(d), hour, minute))
            .find(|t| *t > after)
            .unwrap_or_else(|| after + Duration::days(1))
    };
    match *schedule {
        TaskSchedule::Daily { hour, minute } => daily(hour, minute),
        TaskSchedule::Weekly { weekday, hour, minute } => (0..=8)
            .map(|d| today + Duration::days(d))
            .filter(|date| date.weekday().num_days_from_monday() == u32::from(weekday.min(6)))
            .map(|date| local_time(tz, date, hour, minute))
            .find(|t| *t > after)
            .unwrap_or_else(|| after + Duration::days(7)),
        TaskSchedule::EveryNDays { days, hour, minute } => {
            let days = i64::from(days.max(1));
            match last_run {
                Some(last) => {
                    let base = last.with_timezone(tz).date_naive() + Duration::days(days);
                    let t = local_time(tz, base, hour, minute);
                    if t > after { t } else { daily(hour, minute) }
                }
                None => daily(hour, minute),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono_tz::Europe::London;
    use chrono_tz::US::Eastern;

    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn daily_is_today_or_tomorrow() {
        let s = TaskSchedule::Daily { hour: 9, minute: 0 };
        assert_eq!(next_run(&s, utc("2026-06-10T07:00:00Z"), None, &London), utc("2026-06-10T08:00:00Z"));
        assert_eq!(next_run(&s, utc("2026-06-10T08:00:00Z"), None, &London), utc("2026-06-11T08:00:00Z"), "strictly after");
    }

    #[test]
    fn weekly_finds_the_weekday() {
        // 2026-06-10 is a Wednesday. Friday = 4.
        let s = TaskSchedule::Weekly { weekday: 4, hour: 16, minute: 30 };
        assert_eq!(next_run(&s, utc("2026-06-10T12:00:00Z"), None, &Utc), utc("2026-06-12T16:30:00Z"));
        // Same weekday, after the time: next week.
        let s = TaskSchedule::Weekly { weekday: 2, hour: 9, minute: 0 };
        assert_eq!(next_run(&s, utc("2026-06-10T12:00:00Z"), None, &Utc), utc("2026-06-17T09:00:00Z"));
    }

    #[test]
    fn every_n_days_counts_from_the_last_run() {
        let s = TaskSchedule::EveryNDays { days: 3, hour: 9, minute: 0 };
        let last = utc("2026-06-10T09:00:05Z");
        assert_eq!(next_run(&s, last, Some(last), &Utc), utc("2026-06-13T09:00:00Z"));
        // Missed by a week: the next run is the next 9:00, not a backlog.
        assert_eq!(next_run(&s, utc("2026-06-20T10:00:00Z"), Some(last), &Utc), utc("2026-06-21T09:00:00Z"));
        // Never run: next 9:00.
        assert_eq!(next_run(&s, utc("2026-06-10T08:00:00Z"), None, &Utc), utc("2026-06-10T09:00:00Z"));
    }

    #[test]
    fn spring_forward_gap_moves_to_the_first_valid_time() {
        // London skips 01:00–02:00 on 2026-03-29.
        let s = TaskSchedule::Daily { hour: 1, minute: 30 };
        let next = next_run(&s, utc("2026-03-28T23:00:00Z"), None, &London);
        assert_eq!(next, utc("2026-03-29T01:00:00Z"), "02:00 BST, the first valid minute");
        // And the wall-clock time stays put on the following day.
        assert_eq!(next_run(&s, next, None, &London), utc("2026-03-30T00:30:00Z"));
    }

    #[test]
    fn fall_back_ambiguity_uses_the_first_occurrence() {
        // London repeats 01:00–02:00 on 2026-10-25; 01:30 BST is 00:30 UTC.
        let s = TaskSchedule::Daily { hour: 1, minute: 30 };
        assert_eq!(next_run(&s, utc("2026-10-24T23:00:00Z"), None, &London), utc("2026-10-25T00:30:00Z"));
    }

    #[test]
    fn wall_clock_holds_across_dst_for_every_n_days() {
        // US Eastern springs forward on 2026-03-08. 9:00 EST = 14:00 UTC; 9:00 EDT = 13:00 UTC.
        let s = TaskSchedule::EveryNDays { days: 3, hour: 9, minute: 0 };
        let last = utc("2026-03-06T14:00:00Z");
        assert_eq!(next_run(&s, last, Some(last), &Eastern), utc("2026-03-09T13:00:00Z"));
    }
}
