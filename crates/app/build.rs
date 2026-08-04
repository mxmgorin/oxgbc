//! Stamps what this build is into the binary: the release the source tree claims,
//! the commit it came from, and the day it was built. The About page reads the three
//! back through `env!`.
//!
//! No `cargo:rerun-if-changed`: the default — rerun when anything in this crate
//! changes — is what keeps the commit and the date from going stale while working,
//! and a release build starts from a clean tree anyway.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// The release record. Crate versions are never bumped in this workspace, so what a
/// release is called is what the changelog's newest heading says.
const CHANGELOG: &str = "../../CHANGELOG.md";
/// Stamped in place of a value this build cannot reach: a source tarball carries no
/// git repository.
const UNKNOWN: &str = "unknown";

const SECS_PER_DAY: u64 = 24 * 60 * 60;
/// Days from 0000-03-01, where [`civil_from_days`] counts from, to 1970-01-01.
const EPOCH_SHIFT_DAYS: i64 = 719_468;
/// The Gregorian calendar repeats every 400 years, which is a whole number of days.
const YEARS_PER_ERA: i64 = 400;
const DAYS_PER_ERA: i64 = 146_097;
/// [`civil_from_days`]'s leap-cycle divisors: the days a 4-year, 100-year and
/// 400-year cycle spans, the cycles whose last day is a leap day counted one short so
/// that day falls to the cycle it ends.
const DAYS_PER_4_YEARS: i64 = 4 * 365;
const DAYS_PER_CENTURY: i64 = 100 * 365 + 24;
const DAYS_PER_LEAP_ERA: i64 = DAYS_PER_ERA - 1;

fn main() {
    println!("cargo:rustc-env=OXGBC_VERSION={}", changelog_version());
    println!("cargo:rustc-env=OXGBC_COMMIT={}", commit());
    println!("cargo:rustc-env=OXGBC_BUILD_DATE={}", build_date());
}

/// The newest `## [x.y]` heading of the changelog. A heading not starting on a digit
/// — `[Unreleased]` — is passed over: it is not a name this build could claim.
fn changelog_version() -> String {
    let changelog = std::fs::read_to_string(CHANGELOG).unwrap_or_default();

    changelog
        .lines()
        .filter_map(|line| Some(line.strip_prefix("## [")?.split_once(']')?.0))
        .find(|version| version.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(UNKNOWN)
        .to_owned()
}

fn commit() -> String {
    let git = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    match git {
        Ok(git) if git.status.success() => String::from_utf8_lossy(&git.stdout).trim().to_owned(),
        _ => UNKNOWN.to_owned(),
    }
}

/// The day the build ran, in UTC. `SOURCE_DATE_EPOCH` wins where it is set, so a
/// reproducible build stamps the date it is pinned to rather than today.
fn build_date() -> String {
    let secs = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|epoch| epoch.trim().parse().ok())
        .unwrap_or_else(now_secs);
    let (year, month, day) = civil_from_days((secs / SECS_PER_DAY) as i64);

    format!("{year:04}-{month:02}-{day:02}")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the build clock is not set before 1970")
        .as_secs()
}

/// Days since 1970-01-01 as a calendar date, by Howard Hinnant's `civil_from_days`:
/// counting years from March puts the leap day last, which lets one polynomial give
/// every month its length. Cheaper than pulling in a date crate for one date.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + EPOCH_SHIFT_DAYS;
    let era = shifted.div_euclid(DAYS_PER_ERA);
    let day_of_era = shifted.rem_euclid(DAYS_PER_ERA);
    // Every fourth year is a leap year, bar every hundredth, bar every four-hundredth.
    let year_of_era = (day_of_era - day_of_era / DAYS_PER_4_YEARS + day_of_era / DAYS_PER_CENTURY
        - day_of_era / DAYS_PER_LEAP_ERA)
        / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    // March is month 0 in this count, so January and February belong to the next year.
    let march_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * march_month + 2) / 5 + 1;
    let month = if march_month < 10 {
        march_month + 3
    } else {
        march_month - 9
    };

    (
        year_of_era + era * YEARS_PER_ERA + i64::from(month <= 2),
        month,
        day,
    )
}
