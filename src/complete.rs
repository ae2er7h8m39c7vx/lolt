//! Dynamic shell completion for `--league` and `--team`.
//!
//! Both candidate lists come from the API, which a TAB press must not wait on
//! more than once: results are cached on disk, and a lookup that fails falls
//! back to a stale cache rather than to nothing. Completion is best-effort
//! throughout — every error path yields an empty list instead of noise in the
//! user's shell.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap_complete::engine::CompletionCandidate;
use serde::de::DeserializeOwned;
use serde::Serialize;

use lolt_core::api::{Api, DEFAULT_API_KEY};
use lolt_core::models::{League, StandingTeam};
use lolt_core::resolve::{find_league, league_teams, norm};

/// Shorter than the CLI default: a shell that stops responding is worse than
/// no completion.
const TIMEOUT_SECS: u64 = 5;
/// Leagues are all but static; a split's line-up only changes between splits.
const LEAGUES_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const TEAMS_TTL: Duration = Duration::from_secs(12 * 60 * 60);

/// Candidates for `-l/--league`: league slugs, top tier first.
pub fn leagues(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = current.to_str().map(str::to_lowercase) else {
        return Vec::new();
    };
    let Some(mut leagues) = cached_leagues() else {
        return Vec::new();
    };
    leagues.sort_by(|a, b| (a.priority, &a.name).cmp(&(b.priority, &b.name)));

    leagues
        .iter()
        .filter(|l| l.slug.to_lowercase().starts_with(&prefix))
        .map(|l| candidate(&l.slug, &format!("{} · {}", l.name, l.region)))
        .collect()
}

/// Candidates for `-t/--team`: the teams of whichever league the command line
/// selects, by slug, falling back to the code (`GEN`, `HLE`) when that is what
/// is being typed. Both spellings resolve.
pub fn teams(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = current.to_str().map(str::to_lowercase) else {
        return Vec::new();
    };
    let Some(teams) = cached_teams(&selected_league()) else {
        return Vec::new();
    };

    teams
        .iter()
        .filter_map(|t| {
            let value = if t.slug.to_lowercase().starts_with(&prefix) {
                t.slug.clone()
            } else if t.code.to_lowercase().starts_with(&prefix) {
                t.code.to_lowercase()
            } else {
                return None;
            };
            Some(candidate(&value, &format!("{} · {}", t.code, t.name)))
        })
        .collect()
}

fn candidate(value: &str, help: &str) -> CompletionCandidate {
    CompletionCandidate::new(value).help(Some(help.to_owned().into()))
}

// ------------------------------------------------------------ command line

/// The league the command line being completed asks for.
///
/// The completer is only handed the word under the cursor, but the shell
/// passes the whole line through as arguments, so `-t <TAB>` can still know
/// which league's teams to offer.
fn selected_league() -> String {
    let words: Vec<String> = std::env::args_os()
        .map(|w| w.to_string_lossy().into_owned())
        .collect();
    // Everything after the `--` the completion hook inserts is the user's line.
    let start = words.iter().position(|w| w == "--").map_or(0, |i| i + 1);

    league_flag(&words[start..])
        .or_else(|| std::env::var("LOLT_LEAGUE").ok())
        .unwrap_or_else(|| "lck".to_string())
}

/// Value of `--league`/`-l` in `words`, in any of the spellings clap accepts.
fn league_flag(words: &[String]) -> Option<String> {
    let mut rest = words.iter();
    while let Some(word) = rest.next() {
        let value = if let Some(value) = word.strip_prefix("--league=") {
            Some(value.to_string())
        } else if word == "--league" {
            rest.next().cloned()
        } else if let Some(shorts) = word.strip_prefix('-').filter(|w| !w.starts_with('-')) {
            // `-l lec`, `-llec`, or `-l` at the end of a flag cluster (`-Tl lec`).
            match shorts.split_once('l') {
                Some((_, "")) => rest.next().cloned(),
                Some((_, tail)) => Some(tail.to_string()),
                None => continue,
            }
        } else {
            continue;
        };
        // An empty value is the word still being typed, not a league.
        return value.filter(|v| !v.is_empty());
    }
    None
}

// ------------------------------------------------------------------ lookup

fn cached_leagues() -> Option<Vec<League>> {
    cached("leagues", LEAGUES_TTL, || api()?.leagues().ok())
}

fn cached_teams(league_query: &str) -> Option<Vec<StandingTeam>> {
    let league = find_league(&cached_leagues()?, league_query).ok()?;
    let key = format!("teams-{}", norm(&league.slug));
    cached(&key, TEAMS_TTL, || league_teams(&api()?, &league).ok())
}

fn api() -> Option<Api> {
    let key = std::env::var("LOLESPORTS_API_KEY").unwrap_or_else(|_| DEFAULT_API_KEY.to_string());
    Api::new(&key, "en-US", TIMEOUT_SECS).ok()
}

// ------------------------------------------------------------------- cache

/// Fresh cache, else `fetch`, else whatever stale copy is on disk.
fn cached<T: Serialize + DeserializeOwned>(
    key: &str,
    ttl: Duration,
    fetch: impl FnOnce() -> Option<T>,
) -> Option<T> {
    let path = cache_dir().map(|dir| dir.join(format!("{key}.json")));

    if let Some(fresh) = path.as_deref().and_then(|p| read(p, Some(ttl))) {
        return Some(fresh);
    }
    match fetch() {
        Some(fetched) => {
            if let Some(path) = path.as_deref() {
                write(path, &fetched);
            }
            Some(fetched)
        }
        // Offline, or the API is having a bad day: yesterday's list still beats
        // an empty one.
        None => path.as_deref().and_then(|p| read(p, None)),
    }
}

/// `$XDG_CACHE_HOME/lolt`, or the platform's usual cache location.
fn cache_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(xdg).join("lolt"));
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let base = if cfg!(target_os = "macos") {
        home.join("Library/Caches")
    } else {
        home.join(".cache")
    };
    Some(base.join("lolt"))
}

/// Read `path`, requiring it to be younger than `ttl` when one is given.
fn read<T: DeserializeOwned>(path: &Path, ttl: Option<Duration>) -> Option<T> {
    if let Some(ttl) = ttl {
        let age = std::fs::metadata(path)
            .ok()?
            .modified()
            .ok()?
            .elapsed()
            .ok()?;
        if age > ttl {
            return None;
        }
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Write via a temporary file so a second shell never reads a half-written one.
fn write<T: Serialize>(path: &Path, value: &T) {
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(bytes) = serde_json::to_vec(value) else {
        return;
    };
    let temp = dir.join(format!(".{}.tmp", std::process::id()));
    if std::fs::write(&temp, bytes).is_ok() && std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag(line: &str) -> Option<String> {
        let words: Vec<String> = line.split_whitespace().map(str::to_string).collect();
        league_flag(&words)
    }

    #[test]
    fn league_flag_reads_every_spelling() {
        assert_eq!(flag("lolt -l lec -t g2"), Some("lec".into()));
        assert_eq!(flag("lolt -llec"), Some("lec".into()));
        assert_eq!(flag("lolt --league lec"), Some("lec".into()));
        assert_eq!(flag("lolt --league=lec"), Some("lec".into()));
        assert_eq!(flag("lolt schedule -Tl lec"), Some("lec".into()));
    }

    #[test]
    fn league_flag_ignores_lines_without_one() {
        assert_eq!(flag("lolt -t g2 standings"), None);
        assert_eq!(flag("lolt --json live --all"), None);
        // The flag is there but its value is the word still being typed.
        assert_eq!(flag("lolt -l"), None);
    }
}
