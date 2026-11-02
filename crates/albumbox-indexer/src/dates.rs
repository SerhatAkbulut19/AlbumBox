//! "When was this taken?" — all results are local wall-clock time formatted
//! as `YYYY-MM-DDTHH:MM:SS`.

use std::sync::OnceLock;

use regex::Regex;
use time::{macros::format_description, Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

const OUT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");

fn fmt(dt: PrimitiveDateTime) -> String {
    dt.format(OUT).unwrap_or_default()
}

/// Local UTC offset of this machine (Turkey = +03:00 when unknown).
pub fn local_offset() -> UtcOffset {
    static OFFSET: OnceLock<UtcOffset> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        UtcOffset::current_local_offset().unwrap_or_else(|_| UtcOffset::from_hms(3, 0, 0).unwrap())
    })
}

fn plausible(year: i32) -> bool {
    (1900..=2100).contains(&year)
}

fn build(y: i32, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> Option<PrimitiveDateTime> {
    if !plausible(y) {
        return None;
    }
    let date = Date::from_calendar_date(y, Month::try_from(mo).ok()?, d).ok()?;
    let time = Time::from_hms(h, mi, s).ok()?;
    Some(PrimitiveDateTime::new(date, time))
}

/// EXIF `DateTimeOriginal` ("2022:05:14 13:22:10"; local time by convention).
pub fn from_exif(value: &str) -> Option<String> {
    let v = value.trim().trim_matches('"');
    let digits: Vec<u32> = v
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    if digits.len() < 6 {
        return None;
    }
    build(digits[0] as i32, digits[1] as u8, digits[2] as u8, digits[3] as u8, digits[4] as u8, digits[5] as u8)
        .map(fmt)
}

/// Video `creation_time` (UTC, e.g. "2022-05-14T10:22:10.000000Z") → local.
pub fn from_video_utc(value: &str) -> Option<String> {
    let dt = OffsetDateTime::parse(value.trim(), &time::format_description::well_known::Rfc3339).ok()?;
    if !plausible(dt.year()) || dt.year() <= 1970 {
        return None; // many cameras write 1970/1904 epoch when the clock is unset
    }
    let local = dt.to_offset(local_offset());
    Some(fmt(PrimitiveDateTime::new(local.date(), local.time())))
}

/// Dates embedded in file names:
/// `IMG_20220514_132210`, `VID-20230421-WA0012`, `20220514_132210`,
/// `Screenshot_2022-05-14-13-22-10`, `2022-05-14 13.22.10`, `PXL_20220514_102210123`.
pub fn from_filename(name: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?x)
            (?:^|[^0-9])
            (?P<y>(?:19|20)\d{2}) [-_.]? (?P<mo>0[1-9]|1[0-2]) [-_.]? (?P<d>0[1-9]|[12]\d|3[01])
            (?: [\sT_-]? (?P<h>[01]\d|2[0-3]) [-_.:]? (?P<mi>[0-5]\d) [-_.:]? (?P<s>[0-5]\d) )?
            (?:[^0-9]|\d{3}(?:[^0-9]|$)|$)",
        )
        .unwrap()
    });
    let c = re.captures(name)?;
    let n = |k: &str| c.name(k).map(|m| m.as_str().parse::<u32>().unwrap_or(0));
    build(
        n("y")? as i32,
        n("mo")? as u8,
        n("d")? as u8,
        n("h").unwrap_or(0) as u8,
        n("mi").unwrap_or(0) as u8,
        n("s").unwrap_or(0) as u8,
    )
    .map(fmt)
}

/// File modification time (unix seconds) → local.
pub fn from_mtime(unix: i64) -> Option<String> {
    let dt = OffsetDateTime::from_unix_timestamp(unix).ok()?.to_offset(local_offset());
    plausible(dt.year()).then(|| fmt(PrimitiveDateTime::new(dt.date(), dt.time())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exif_dates() {
        assert_eq!(from_exif("2022:05:14 13:22:10").as_deref(), Some("2022-05-14T13:22:10"));
        assert_eq!(from_exif("0000:00:00 00:00:00"), None);
        assert_eq!(from_exif("    :  :     :  :  "), None);
    }

    #[test]
    fn filename_dates() {
        let cases = [
            ("IMG_20220514_132210.jpg", Some("2022-05-14T13:22:10")),
            ("VID-20230421-WA0012.mp4", Some("2023-04-21T00:00:00")),
            ("20220514_132210.jpg", Some("2022-05-14T13:22:10")),
            ("Screenshot_2022-05-14-13-22-10.png", Some("2022-05-14T13:22:10")),
            ("2022-05-14 13.22.10.jpg", Some("2022-05-14T13:22:10")),
            ("PXL_20220514_102210123.jpg", Some("2022-05-14T10:22:10")),
            ("Nişan 2022.jpg", None),
            ("DSC_0412.JPG", None),
            ("IMG_20221345_000000.jpg", None),
        ];
        for (name, expected) in cases {
            assert_eq!(from_filename(name).as_deref(), expected, "{name}");
        }
    }

    #[test]
    fn video_dates() {
        assert!(from_video_utc("2022-05-14T10:22:10.000000Z").unwrap().starts_with("2022-05-14T"));
        assert_eq!(from_video_utc("1970-01-01T00:00:00Z"), None);
    }
}
