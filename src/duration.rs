//! Durations typed by staff (`30m`, `2h`, `7d`, `1d12h`; units `s`, `m`, `h`, `d`, `w`, the
//! same in every language because they are command arguments) and how they are shown, plus the
//! UTC clock the history and the audit log print.

use crate::lang;

/// The longest duration accepted: ten years.
pub const MAX_SECONDS: u64 = 10 * 365 * 86_400;

const UNITS: [(char, u64, &str); 5] = [
    ('w', 7 * 86_400, "unit_w"),
    ('d', 86_400, "unit_d"),
    ('h', 3_600, "unit_h"),
    ('m', 60, "unit_m"),
    ('s', 1, "unit_s"),
];

/// Seconds of a duration such as `1d12h`; `None` for anything else (empty, zero, no unit, an
/// unknown unit, a sign, a space, a fraction, more than ten years).
#[must_use]
pub fn parse(input: &str) -> Option<u64> {
    let mut total: u64 = 0;
    let mut number: Option<u64> = None;
    let mut digits = 0;
    for c in input.chars() {
        if let Some(digit) = c.to_digit(10).filter(|_| c.is_ascii_digit()) {
            digits += 1;
            if digits > 9 {
                return None;
            }
            number = Some(number.unwrap_or(0) * 10 + u64::from(digit));
        } else {
            let lower = c.to_ascii_lowercase();
            let (_, size, _) = UNITS.iter().find(|(unit, _, _)| *unit == lower)?;
            let count = number.take()?;
            digits = 0;
            total = total.checked_add(count.checked_mul(*size)?)?;
        }
    }
    if number.is_some() || total == 0 || total > MAX_SECONDS {
        return None;
    }
    Some(total)
}

/// The two largest non-zero units of `seconds`, as `(count, unit key)`.
#[must_use]
pub fn split(seconds: u64) -> Vec<(u64, &'static str)> {
    if seconds == 0 {
        return vec![(0, "unit_s")];
    }
    let mut left = seconds;
    let mut out = Vec::with_capacity(2);
    for (_, size, key) in UNITS {
        let count = left / size;
        if count > 0 && out.len() < 2 {
            out.push((count, key));
        }
        left %= size;
    }
    out
}

/// `seconds` with the unit names of `language`, for example `1h 30m`.
#[must_use]
pub fn render(language: &str, seconds: u64) -> String {
    split(seconds)
        .into_iter()
        .map(|(count, key)| {
            lang::Msg::new(key)
                .plain(count.to_string())
                .render(language)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Year, month, day, hour, minute and second (UTC) of a Unix time.
#[must_use]
pub fn civil(seconds: u64) -> (i64, u32, u32, u32, u32, u32) {
    // Days to a date, after Howard Hinnant's `civil_from_days` (public domain algorithm).
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let narrow = |v: i64| u32::try_from(v).unwrap_or(0);
    let narrow_u = |v: u64| u32::try_from(v).unwrap_or(0);
    (
        year,
        narrow(month),
        narrow(day),
        narrow_u(rest / 3_600),
        narrow_u(rest % 3_600 / 60),
        narrow_u(rest % 60),
    )
}

/// `2026-11-02 03:04` (UTC), as the history shows it.
#[must_use]
pub fn stamp(seconds: u64) -> String {
    let (y, mo, d, h, mi, _) = civil(seconds);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
}

/// `2026-11-02T03:04:05Z`, as the audit log writes it.
#[must_use]
pub fn iso(seconds: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(seconds);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Seconds since 1970 now (0 if the clock is before it).
#[must_use]
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_parse_with_every_unit_and_in_combinations() {
        assert_eq!(parse("30s"), Some(30));
        assert_eq!(parse("30m"), Some(1_800));
        assert_eq!(parse("2h"), Some(7_200));
        assert_eq!(parse("1H"), Some(3_600));
        assert_eq!(parse("7d"), Some(604_800));
        assert_eq!(parse("2w"), Some(1_209_600));
        assert_eq!(parse("1d12h"), Some(129_600));
        assert_eq!(parse("1h30m15s"), Some(5_415));
    }

    #[test]
    fn anything_else_is_not_a_duration() {
        for bad in [
            "",
            "0m",
            "m",
            "5",
            "5x",
            "-5m",
            "+5m",
            "1.5h",
            "1d 2h",
            " 1d",
            "1dd",
            "d1",
            "99999999999999999999d",
            "3651d",
            "１h",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
        assert_eq!(parse("3650d"), Some(MAX_SECONDS));
    }

    #[test]
    fn durations_show_their_two_largest_units() {
        assert_eq!(split(90_061), vec![(1, "unit_d"), (1, "unit_h")]);
        assert_eq!(split(59), vec![(59, "unit_s")]);
        assert_eq!(split(3_600), vec![(1, "unit_h")]);
        assert_eq!(split(1_209_660), vec![(2, "unit_w"), (1, "unit_m")]);
        assert_eq!(split(0), vec![(0, "unit_s")]);
        assert_eq!(render("en", 5_400), "1h 30m");
        assert_eq!(render("fr", 86_400), "1 j");
    }

    #[test]
    fn the_clock_prints_utc_dates() {
        assert_eq!(stamp(0), "1970-01-01 00:00");
        assert_eq!(iso(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(stamp(951_782_400), "2000-02-29 00:00");
        assert_eq!(iso(1_793_588_645), "2026-11-02T03:04:05Z");
        assert_eq!(stamp(4_102_444_799), "2099-12-31 23:59");
    }
}
