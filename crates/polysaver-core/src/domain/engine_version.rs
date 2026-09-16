// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! Pure helpers to reason about the calendar versioning used by yt-dlp.
//!
//! yt-dlp releases are versioned as `YYYY.MM.DD[.REV]` (e.g. `2026.08.19` or
//! `2026.08.30.232658` for nightly builds). The upstream project itself warns
//! when a release is older than 90 days, which is the threshold used here.

/// Number of days after which a yt-dlp release is considered outdated.
/// Matches the warning threshold used by yt-dlp itself.
pub const OUTDATED_THRESHOLD_DAYS: i64 = 90;

/// Days elapsed since the date encoded in a yt-dlp version (`YYYY.MM.DD...`).
/// Returns `None` when the version does not follow the calendar format.
#[must_use]
pub fn version_age_days(version: &str, today: (i32, u32, u32)) -> Option<i64> {
    let trimmed = version.trim();
    let mut parts = trimmed.split('.');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Reject versions that are not calendar-like (e.g. single-component strings).
    if parts.next().is_some() {
        // Trailing revision components (`YYYY.MM.DD.REV`) are accepted and ignored.
        if parts.any(|p| p.is_empty()) {
            return None;
        }
    }

    let released = days_from_civil(year, month, day);
    let today_days = days_from_civil(today.0, today.1, today.2);
    Some(today_days - released)
}

/// Returns whether a version is older than [`OUTDATED_THRESHOLD_DAYS`].
/// Non-calendar versions cannot be judged and are reported as not outdated.
#[must_use]
pub fn is_version_outdated(version: &str, today: (i32, u32, u32)) -> bool {
    matches!(version_age_days(version, today), Some(age) if age > OUTDATED_THRESHOLD_DAYS)
}

/// Ordering of two yt-dlp versions, comparing dot-separated numeric components.
/// Missing revision components compare as zero (`2026.08.19` < `2026.08.19.1`),
/// so both `YYYY.MM.DD` and `YYYY.MM.DD.REV` (nightly) shapes are ordered correctly.
/// Returns `None` when either version is not numerically comparable.
#[must_use]
pub fn compare_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let parse = |v: &str| -> Option<Vec<u64>> {
        let parts: Option<Vec<u64>> = v.trim().split('.').map(|p| p.parse::<u64>().ok()).collect();
        let parts = parts?;
        if parts.is_empty() {
            None
        } else {
            Some(parts)
        }
    };

    let a = parse(left)?;
    let b = parse(right)?;
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            std::cmp::Ordering::Equal => {}
            other => return Some(other),
        }
    }
    Some(std::cmp::Ordering::Equal)
}

/// Current UTC date as `(year, month, day)`, derived from the system clock.
/// Injected explicitly into the pure helpers to keep them testable.
#[must_use]
pub fn current_utc_date() -> (i32, u32, u32) {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as i64)
        .unwrap_or(0);
    civil_from_days(days)
}

/// Inverse of [`days_from_civil`]: civil date from days since 1970-01-01.
#[must_use]
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m as u32, d as u32)
}

/// Howard Hinnant's civil date algorithm: days since 1970-01-01.
/// Arithmetic only, so the core keeps no date/time dependency.
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let m = i64::from(m);
    let d = i64::from(d);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_age_days_calendar_formats() {
        assert_eq!(version_age_days("2026.08.19", (2026, 9, 13)), Some(25));
        // Nightly build with revision suffix is still calendar-versioned.
        assert_eq!(
            version_age_days("2026.08.30.232658", (2026, 9, 13)),
            Some(14)
        );
        // Same day.
        assert_eq!(version_age_days("2026.09.13", (2026, 9, 13)), Some(0));
        // Across a year boundary.
        assert_eq!(version_age_days("2025.12.31", (2026, 1, 1)), Some(1));
    }

    #[test]
    fn test_version_age_days_rejects_non_calendar_versions() {
        assert_eq!(version_age_days("nightly", (2026, 9, 13)), None);
        assert_eq!(version_age_days("2026", (2026, 9, 13)), None);
        assert_eq!(version_age_days("2026.13.01", (2026, 9, 13)), None);
        assert_eq!(version_age_days("2026.08.32", (2026, 9, 13)), None);
        assert_eq!(version_age_days("", (2026, 9, 13)), None);
    }

    #[test]
    fn test_is_version_outdated_threshold() {
        // Exactly at the threshold is not "older than".
        assert!(!is_version_outdated("2026.09.13", (2026, 9, 13)));
        assert!(!is_version_outdated("2026.06.15", (2026, 9, 13))); // 90 days
        assert!(is_version_outdated("2026.06.14", (2026, 9, 13))); // 91 days
        assert!(!is_version_outdated("not-a-date", (2026, 9, 13)));
    }

    #[test]
    fn test_compare_versions_stable_and_nightly() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_versions("2026.08.30.232658", "2026.08.19"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_versions("2026.08.19", "2026.08.19"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_versions("2026.08.19", "2026.08.19.1"),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_versions("2025.12.31", "2026.01.01"),
            Some(Ordering::Less)
        );
        assert_eq!(compare_versions("nightly", "2026.08.19"), None);
    }

    #[test]
    fn test_civil_round_trip() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
        assert_eq!(days_from_civil(2024, 10, 4), 20_000);
        let (y, m, d) = current_utc_date();
        assert!(y >= 2026 && (1..=12).contains(&m) && (1..=31).contains(&d));
    }
}
