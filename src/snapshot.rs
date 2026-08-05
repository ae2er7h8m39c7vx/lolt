//! The shared view model.
//!
//! Both the CLI renderer and the macOS widget consume [`Snapshot`]: the CLI
//! prints it, the widget receives it as JSON across the C ABI in [`crate::ffi`].
//! Keeping one representation means the two front-ends can never disagree about
//! what "T1's next match" is.

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::api::Api;
use crate::models::{Event, League, Record, Standing, Team, Tournament};
use crate::resolve::{current_tournament, find_league, norm, resolve_team};

/// Timestamps cross the FFI boundary at whole-second precision, so Swift's
/// stock `JSONDecoder.DateDecodingStrategy.iso8601` can read them — it rejects
/// the fractional seconds `Utc::now()` would otherwise emit.
mod rfc3339_secs {
    use super::*;

    pub fn serialize<S: Serializer>(t: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&t.to_rfc3339_opts(SecondsFormat::Secs, true))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<DateTime<Utc>, D::Error> {
        DateTime::<Utc>::deserialize(d)
    }
}

/// One side of a match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Side {
    pub name: String,
    pub code: String,
    /// Games won in the series so far.
    pub score: i64,
    pub record: Option<Record>,
}

impl Side {
    fn tbd() -> Self {
        Self {
            name: "TBD".to_string(),
            code: "TBD".to_string(),
            score: 0,
            record: None,
        }
    }
}

/// A match, flattened and (optionally) oriented around a tracked team.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchInfo {
    #[serde(with = "rfc3339_secs")]
    pub start_time: DateTime<Utc>,
    /// `unstarted` | `inProgress` | `completed`
    pub state: String,
    pub block: String,
    pub league: String,
    pub league_slug: String,
    pub best_of: Option<i64>,
    pub home: Side,
    pub away: Side,
    /// `win` / `loss` from the tracked team's point of view, once decided.
    pub outcome: Option<String>,
    /// The other side, when a tracked team is playing.
    pub opponent: Option<Side>,
    /// Whether the tracked team is involved at all.
    pub involves_focus: bool,
}

impl MatchInfo {
    /// `None` for non-match events such as opening shows.
    pub fn from_event(event: &Event, focus: Option<&Team>) -> Option<Self> {
        let teams = event.teams();
        let side = |i: usize| {
            teams.get(i).map_or_else(Side::tbd, |t| Side {
                name: if t.name.is_empty() {
                    t.code.clone()
                } else {
                    t.name.clone()
                },
                code: t.code.clone(),
                score: t.result.as_ref().map_or(0, |r| r.game_wins),
                record: t.record,
            })
        };
        event.match_info.as_ref()?;
        let (home, away) = (side(0), side(1));

        let mut outcome = None;
        let mut opponent = None;
        let mut involves_focus = false;
        if let Some(f) = focus {
            let is_focus =
                |s: &Side| norm(&s.code) == norm(&f.code) || norm(&s.name) == norm(&f.name);
            if is_focus(&home) {
                involves_focus = true;
                opponent = Some(away.clone());
            } else if is_focus(&away) {
                involves_focus = true;
                opponent = Some(home.clone());
            }
            if involves_focus {
                outcome = teams
                    .iter()
                    .find(|t| norm(&t.code) == norm(&f.code) || norm(&t.name) == norm(&f.name))
                    .and_then(|t| t.result.as_ref())
                    .and_then(|r| r.outcome.clone());
            }
        }

        let (league, league_slug) = event
            .league
            .as_ref()
            .map(|l| (l.name.clone(), l.slug.clone()))
            .unwrap_or_default();

        Some(Self {
            start_time: event.start_time,
            state: event.state.clone(),
            block: event.block_name.clone().unwrap_or_default(),
            league,
            league_slug,
            best_of: event.best_of(),
            home,
            away,
            outcome,
            opponent,
            involves_focus,
        })
    }

    pub fn is_live(&self) -> bool {
        self.state == "inProgress"
    }

    pub fn is_completed(&self) -> bool {
        self.state == "completed"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueInfo {
    pub slug: String,
    pub name: String,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamInfo {
    pub slug: String,
    pub name: String,
    pub code: String,
    /// Team logo, for the widget to load.
    pub image: Option<String>,
}

/// Where the tracked team sits in its group/table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandingLine {
    pub ordinal: i64,
    pub section: String,
    pub wins: i64,
    pub losses: i64,
}

/// Everything a "how is my team doing" view needs, in one payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(with = "rfc3339_secs")]
    pub generated_at: DateTime<Utc>,
    pub league: LeagueInfo,
    pub team: TeamInfo,
    pub split: Option<String>,
    pub standing: Option<StandingLine>,
    pub live: Vec<MatchInfo>,
    pub upcoming: Vec<MatchInfo>,
    pub recent: Vec<MatchInfo>,
}

impl Snapshot {
    /// The match to lead with: live if one is running, else the next fixture.
    pub fn headline(&self) -> Option<&MatchInfo> {
        self.live.first().or_else(|| self.upcoming.first())
    }
}

/// Fetch and assemble a snapshot for `team_query` in `league_query`.
pub fn build(
    api: &Api,
    league_query: &str,
    team_query: &str,
    upcoming: usize,
    recent: usize,
) -> Result<Snapshot> {
    let league = find_league(&api.leagues()?, league_query)?;
    let team = resolve_team(api, &league, team_query)?;

    let tournaments = api.tournaments(&league.id)?;
    let split = current_tournament(&tournaments);
    let standing = split
        .as_ref()
        .map(|t| api.standings(&t.id))
        .transpose()?
        .and_then(|st| find_standing(&st, &team));

    let events = fetch_events(api, &league.id, 1)?;
    let mine: Vec<MatchInfo> = events
        .iter()
        .filter_map(|e| MatchInfo::from_event(e, Some(&team)))
        .filter(|m| m.involves_focus)
        .collect();

    let mut past: Vec<MatchInfo> = mine.iter().filter(|m| m.is_completed()).cloned().collect();
    past.reverse();
    past.truncate(recent);

    Ok(Snapshot {
        generated_at: Utc::now(),
        league: LeagueInfo {
            slug: league.slug.clone(),
            name: league.name.clone(),
            region: league.region.clone(),
        },
        team: TeamInfo {
            slug: team.slug.clone(),
            name: team.name.clone(),
            code: team.code.clone(),
            image: team.image.clone(),
        },
        split: split.as_ref().map(|t| split_label(t, &league)),
        standing,
        live: mine.iter().filter(|m| m.is_live()).cloned().collect(),
        upcoming: mine
            .iter()
            .filter(|m| !m.is_completed() && !m.is_live())
            .take(upcoming)
            .cloned()
            .collect(),
        recent: past,
    })
}

/// Current page of the schedule plus `older_pages` earlier pages, in
/// chronological order with duplicate matches removed.
pub fn fetch_events(api: &Api, league_id: &str, older_pages: usize) -> Result<Vec<Event>> {
    let first = api.schedule(league_id, None)?;
    let mut token = first.pages.older.clone();
    let mut events = first.events;

    for _ in 0..older_pages {
        let Some(t) = token.take() else { break };
        let page = api.schedule(league_id, Some(&t))?;
        token = page.pages.older.clone();
        events.extend(page.events);
    }

    events.sort_by_key(|e| e.start_time);
    let mut seen = Vec::new();
    events.retain(|e| {
        let key = e
            .match_info
            .as_ref()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| format!("{}-{}", e.start_time, e.kind));
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
    Ok(events)
}

pub fn find_standing(standings: &[Standing], team: &Team) -> Option<StandingLine> {
    for stage in standings.iter().flat_map(|s| &s.stages) {
        for section in &stage.sections {
            for ranking in &section.rankings {
                for t in &ranking.teams {
                    if norm(&t.code) == norm(&team.code) || t.id == team.id {
                        let r = t.record.unwrap_or(Record { wins: 0, losses: 0 });
                        return Some(StandingLine {
                            ordinal: ranking.ordinal,
                            section: section.name.clone(),
                            wins: r.wins,
                            losses: r.losses,
                        });
                    }
                }
            }
        }
    }
    None
}

/// `lck_split_3_2026` + league `lck` -> `Split 3 2026`; the league name is
/// already on screen, so the redundant prefix is dropped.
pub fn split_label(tournament: &Tournament, league: &League) -> String {
    let slug = tournament
        .slug
        .strip_prefix(&format!("{}_", league.slug))
        .or_else(|| tournament.slug.strip_prefix(&format!("{}-", league.slug)))
        .unwrap_or(&tournament.slug);
    pretty_slug(slug)
}

/// `split_3_2026` -> `Split 3 2026`
pub fn pretty_slug(slug: &str) -> String {
    slug.split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn league() -> League {
        League {
            id: "1".into(),
            slug: "lck".into(),
            name: "LCK".into(),
            region: "KOREA".into(),
            priority: 1,
        }
    }

    #[test]
    fn split_label_drops_the_league_prefix() {
        let tournament = Tournament {
            id: "2".into(),
            slug: "lck_split_3_2026".into(),
            start_date: NaiveDate::from_ymd_opt(2026, 7, 28).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
        };
        assert_eq!(split_label(&tournament, &league()), "Split 3 2026");
    }

    #[test]
    fn match_info_orients_around_the_tracked_team() {
        let json = serde_json::json!({
            "startTime": "2026-08-05T08:00:00Z",
            "state": "completed",
            "type": "match",
            "blockName": "Week 11",
            "league": { "name": "LCK", "slug": "lck" },
            "match": {
                "id": "1",
                "teams": [
                    { "name": "Gen.G Esports", "code": "GEN",
                      "result": { "outcome": "loss", "gameWins": 0 } },
                    { "name": "T1", "code": "T1",
                      "result": { "outcome": "win", "gameWins": 2 } }
                ],
                "strategy": { "type": "bestOf", "count": 3 }
            }
        });
        let event: Event = serde_json::from_value(json).unwrap();
        let t1 = Team {
            id: "t1".into(),
            slug: "t1".into(),
            name: "T1".into(),
            code: "T1".into(),
            image: None,
            status: None,
            home_league: None,
            players: vec![],
        };

        let m = MatchInfo::from_event(&event, Some(&t1)).unwrap();
        assert!(m.involves_focus);
        assert_eq!(m.outcome.as_deref(), Some("win"));
        assert_eq!(m.opponent.unwrap().code, "GEN");
        assert_eq!(m.away.score, 2);
        assert_eq!(m.best_of, Some(3));

        // Without a tracked team there is no point of view to report.
        let neutral = MatchInfo::from_event(&event, None).unwrap();
        assert!(!neutral.involves_focus);
        assert!(neutral.outcome.is_none());
    }

    #[test]
    fn show_events_are_not_matches() {
        let json = serde_json::json!({
            "startTime": "2026-08-05T08:00:00Z",
            "state": "unstarted",
            "type": "show",
            "league": { "name": "LCK", "slug": "lck" }
        });
        let event: Event = serde_json::from_value(json).unwrap();
        assert!(MatchInfo::from_event(&event, None).is_none());
    }
}
