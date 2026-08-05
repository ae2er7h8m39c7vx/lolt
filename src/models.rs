//! Serde types for the lolesports.com esports API.
//!
//! Every endpoint wraps its payload in `{"data": ...}`, hence [`Envelope`].
//! Fields the API sometimes omits are `Option`/`default` so a schema tweak
//! upstream degrades a single column instead of failing the whole command.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Envelope<T> {
    pub data: T,
}

// ---------------------------------------------------------------- leagues

#[derive(Debug, Deserialize)]
pub struct LeaguesData {
    pub leagues: Vec<League>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct League {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub priority: i64,
}

// --------------------------------------------------------------- schedule

#[derive(Debug, Deserialize)]
pub struct ScheduleData {
    pub schedule: Schedule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    #[serde(default)]
    pub pages: Pages,
    #[serde(default)]
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pages {
    #[serde(default)]
    pub older: Option<String>,
    #[serde(default)]
    pub newer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub start_time: DateTime<Utc>,
    /// `unstarted` | `inProgress` | `completed`
    #[serde(default)]
    pub state: String,
    /// `match` | `show`
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub block_name: Option<String>,
    #[serde(default)]
    pub league: Option<EventLeague>,
    /// Absent on `show` events (opening ceremonies and the like).
    #[serde(default, rename = "match")]
    pub match_info: Option<Match>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLeague {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    pub id: String,
    #[serde(default)]
    pub teams: Vec<MatchTeam>,
    #[serde(default)]
    pub strategy: Option<Strategy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchTeam {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub result: Option<MatchResult>,
    #[serde(default)]
    pub record: Option<Record>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchResult {
    /// `win` | `loss` | absent while a series is still running
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub game_wins: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Record {
    #[serde(default)]
    pub wins: i64,
    #[serde(default)]
    pub losses: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub count: i64,
}

impl Event {
    pub fn is_live(&self) -> bool {
        self.state == "inProgress"
    }

    pub fn is_completed(&self) -> bool {
        self.state == "completed"
    }

    /// Best-of length, when the API reports one.
    pub fn best_of(&self) -> Option<i64> {
        let s = self.match_info.as_ref()?.strategy.as_ref()?;
        (s.kind == "bestOf").then_some(s.count)
    }

    pub fn teams(&self) -> &[MatchTeam] {
        self.match_info.as_ref().map_or(&[], |m| &m.teams)
    }

    /// True when the given team is one of the two sides.
    pub fn involves(&self, team: &Team) -> bool {
        let (code, name) = (
            crate::resolve::norm(&team.code),
            crate::resolve::norm(&team.name),
        );
        self.teams()
            .iter()
            .any(|t| crate::resolve::norm(&t.code) == code || crate::resolve::norm(&t.name) == name)
    }
}

// ------------------------------------------------------------------ teams

#[derive(Debug, Deserialize)]
pub struct TeamsData {
    /// Unknown ids come back as `null` entries rather than being dropped.
    #[serde(default)]
    pub teams: Vec<Option<Team>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub home_league: Option<HomeLeague>,
    #[serde(default)]
    pub players: Vec<Player>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeLeague {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    #[serde(default)]
    pub summoner_name: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub role: String,
}

// ------------------------------------------------------------ tournaments

#[derive(Debug, Deserialize)]
pub struct TournamentsData {
    pub leagues: Vec<LeagueTournaments>,
}

#[derive(Debug, Deserialize)]
pub struct LeagueTournaments {
    #[serde(default)]
    pub tournaments: Vec<Tournament>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tournament {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

// -------------------------------------------------------------- standings

#[derive(Debug, Deserialize)]
pub struct StandingsData {
    #[serde(default)]
    pub standings: Vec<Standing>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Standing {
    #[serde(default)]
    pub stages: Vec<Stage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub rankings: Vec<Ranking>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ranking {
    #[serde(default)]
    pub ordinal: i64,
    #[serde(default)]
    pub teams: Vec<StandingTeam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandingTeam {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub record: Option<Record>,
}
