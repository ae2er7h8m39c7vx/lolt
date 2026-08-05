//! Thin blocking client over the public lolesports.com esports API.

use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;

use crate::models::*;

const BASE: &str = "https://esports-api.lolesports.com/persisted/gw";

/// Public key shipped in the lolesports.com web client. Override with
/// `--api-key` or `LOLESPORTS_API_KEY` if Riot ever rotates it.
pub const DEFAULT_API_KEY: &str = "0TvQnueqKa5mxJntVWt0w4LpLfEkrV1Ta8rQBb9Z";

pub struct Api {
    client: Client,
    locale: String,
}

impl Api {
    pub fn new(api_key: &str, locale: &str, timeout_secs: u64) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(api_key).context("invalid API key")?,
        );
        let client = Client::builder()
            .user_agent(concat!("lolt/", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()?;
        Ok(Self {
            client,
            locale: locale.to_string(),
        })
    }

    fn get<T: DeserializeOwned>(&self, path: &str, params: &[(&str, &str)]) -> Result<T> {
        let url = format!("{BASE}/{path}");
        let mut query: Vec<(&str, &str)> = vec![("hl", self.locale.as_str())];
        query.extend_from_slice(params);

        let resp = self
            .client
            .get(&url)
            .query(&query)
            .send()
            .with_context(|| format!("request to {path} failed"))?;

        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        if !status.is_success() {
            let snippet: String = body.chars().take(200).collect();
            bail!("{path} returned HTTP {status}: {snippet}");
        }

        let envelope: Envelope<T> = serde_json::from_str(&body)
            .with_context(|| format!("could not parse the {path} response"))?;
        Ok(envelope.data)
    }

    pub fn leagues(&self) -> Result<Vec<League>> {
        let data: LeaguesData = self.get("getLeagues", &[])?;
        Ok(data.leagues)
    }

    /// One page of ~80 events centred on the current week. `page_token` walks
    /// backwards/forwards using the tokens from a previous response.
    pub fn schedule(&self, league_id: &str, page_token: Option<&str>) -> Result<Schedule> {
        let mut params = vec![("leagueId", league_id)];
        if let Some(token) = page_token {
            params.push(("pageToken", token));
        }
        let data: ScheduleData = self.get("getSchedule", &params)?;
        Ok(data.schedule)
    }

    /// Matches happening right now, across every league.
    pub fn live(&self) -> Result<Vec<Event>> {
        let data: ScheduleData = self.get("getLive", &[])?;
        Ok(data.schedule.events)
    }

    pub fn tournaments(&self, league_id: &str) -> Result<Vec<Tournament>> {
        let data: TournamentsData =
            self.get("getTournamentsForLeague", &[("leagueId", league_id)])?;
        Ok(data
            .leagues
            .into_iter()
            .flat_map(|l| l.tournaments)
            .collect())
    }

    pub fn standings(&self, tournament_id: &str) -> Result<Vec<Standing>> {
        let data: StandingsData = self.get("getStandings", &[("tournamentId", tournament_id)])?;
        Ok(data.standings)
    }

    /// Exact slug lookup, e.g. `t1` or `hanwha-life-esports`.
    pub fn team_by_slug(&self, slug: &str) -> Result<Option<Team>> {
        let data: TeamsData = self.get("getTeams", &[("id", slug)])?;
        Ok(data.teams.into_iter().flatten().next())
    }

    /// Every team the API knows about (~1.5 MB); used only as a last-resort
    /// fallback when a name can't be resolved inside the selected league.
    pub fn all_teams(&self) -> Result<Vec<Team>> {
        let data: TeamsData = self.get("getTeams", &[])?;
        Ok(data.teams.into_iter().flatten().collect())
    }
}
