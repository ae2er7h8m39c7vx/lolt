//! Turning what the user typed ("lck", "Gen.G", "t1") into API entities.

use anyhow::{anyhow, bail, Result};
use chrono::Local;

use crate::api::Api;
use crate::models::{League, StandingTeam, Team, Tournament};

/// Case/punctuation-insensitive key: "Gen.G Esports" and "geng" both collapse
/// to comparable forms, so `contains` matching behaves predictably.
pub fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Slug form of free text, for a direct `getTeams?id=` attempt.
fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Match a league by slug, name or region — exact first, then prefix/substring.
pub fn find_league(leagues: &[League], query: &str) -> Result<League> {
    let q = norm(query);
    let exact = leagues
        .iter()
        .find(|l| norm(&l.slug) == q || norm(&l.name) == q);
    if let Some(l) = exact {
        return Ok(l.clone());
    }

    let mut partial: Vec<&League> = leagues
        .iter()
        .filter(|l| {
            norm(&l.slug).starts_with(&q) || norm(&l.name).contains(&q) || norm(&l.region) == q
        })
        .collect();
    // Prefer top-tier leagues when a prefix matches several (lck vs lck_challengers).
    partial.sort_by_key(|l| (l.priority, norm(&l.slug).len()));

    match partial.first() {
        Some(l) => Ok((*l).clone()),
        None => {
            let mut names: Vec<&str> = leagues.iter().map(|l| l.slug.as_str()).collect();
            names.sort_unstable();
            bail!(
                "no league matches '{query}'. Try one of: {}",
                names.join(", ")
            )
        }
    }
}

/// The tournament (split) that covers today, else the most recent one.
pub fn current_tournament(tournaments: &[Tournament]) -> Option<Tournament> {
    if tournaments.is_empty() {
        return None;
    }
    let today = Local::now().date_naive();
    let mut sorted: Vec<&Tournament> = tournaments.iter().collect();
    sorted.sort_by_key(|t| t.start_date);

    sorted
        .iter()
        .rev()
        .find(|t| t.start_date <= today && today <= t.end_date)
        .or_else(|| sorted.iter().rev().find(|t| t.start_date <= today))
        .or(sorted.last())
        .map(|t| (*t).clone())
}

/// Teams competing in a league's current split, from its standings.
pub fn league_teams(api: &Api, league: &League) -> Result<Vec<StandingTeam>> {
    let tournaments = api.tournaments(&league.id)?;
    let Some(tournament) = current_tournament(&tournaments) else {
        return Ok(Vec::new());
    };
    let standings = api.standings(&tournament.id)?;

    let mut teams: Vec<StandingTeam> = Vec::new();
    for team in standings
        .iter()
        .flat_map(|s| &s.stages)
        .flat_map(|s| &s.sections)
        .flat_map(|s| &s.rankings)
        .flat_map(|r| &r.teams)
    {
        if !teams.iter().any(|t| t.id == team.id) {
            teams.push(team.clone());
        }
    }
    Ok(teams)
}

/// Resolve a team: exact slug, then the league's roster, then a global search.
pub fn resolve_team(api: &Api, league: &League, query: &str) -> Result<Team> {
    let slug = slugify(query);
    if !slug.is_empty() {
        if let Some(team) = api.team_by_slug(&slug)? {
            return Ok(team);
        }
    }

    let q = norm(query);
    let in_league = league_teams(api, league)?;
    if let Some(hit) = pick(in_league.iter().map(|t| (&t.slug, &t.name, &t.code)), &q) {
        if let Some(team) = api.team_by_slug(hit)? {
            return Ok(team);
        }
    }

    let all = api.all_teams()?;
    if let Some(hit) = pick(all.iter().map(|t| (&t.slug, &t.name, &t.code)), &q) {
        if let Some(team) = all.iter().find(|t| &t.slug == hit) {
            return Ok(team.clone());
        }
    }

    let mut known: Vec<&str> = in_league.iter().map(|t| t.code.as_str()).collect();
    known.sort_unstable();
    Err(anyhow!(
        "no team matches '{query}'. Teams in {}: {}",
        league.name,
        if known.is_empty() {
            "unknown".to_string()
        } else {
            known.join(", ")
        }
    ))
}

/// Exact code/slug/name match wins over a substring hit on the name.
fn pick<'a, I>(candidates: I, q: &str) -> Option<&'a String>
where
    I: Iterator<Item = (&'a String, &'a String, &'a String)> + Clone,
{
    candidates
        .clone()
        .find(|(slug, name, code)| norm(slug) == q || norm(code) == q || norm(name) == q)
        .or_else(|| {
            candidates
                .clone()
                .find(|(slug, name, _)| norm(name).contains(q) || norm(slug).starts_with(q))
        })
        .map(|(slug, _, _)| slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn league(slug: &str, name: &str, priority: i64) -> League {
        League {
            id: slug.to_string(),
            slug: slug.to_string(),
            name: name.to_string(),
            region: "KOREA".to_string(),
            priority,
        }
    }

    fn tournament(slug: &str, start: &str, end: &str) -> Tournament {
        Tournament {
            id: slug.to_string(),
            slug: slug.to_string(),
            start_date: NaiveDate::parse_from_str(start, "%Y-%m-%d").unwrap(),
            end_date: NaiveDate::parse_from_str(end, "%Y-%m-%d").unwrap(),
        }
    }

    #[test]
    fn norm_ignores_case_and_punctuation() {
        assert_eq!(norm("Gen.G Esports"), "gengesports");
        assert_eq!(norm("T1"), "t1");
    }

    #[test]
    fn slugify_builds_api_ids() {
        assert_eq!(slugify("Hanwha Life Esports"), "hanwha-life-esports");
        assert_eq!(slugify("  T1  "), "t1");
        assert_eq!(slugify("Gen.G"), "gen-g");
    }

    #[test]
    fn league_lookup_prefers_the_top_tier_on_a_prefix_match() {
        let leagues = vec![
            league("lck_challengers_league", "LCK Challengers", 1),
            league("lck", "LCK", 1),
        ];
        assert_eq!(find_league(&leagues, "lck").unwrap().slug, "lck");
        assert_eq!(
            find_league(&leagues, "LCK Challengers").unwrap().slug,
            "lck_challengers_league"
        );
    }

    #[test]
    fn league_lookup_reports_unknown_names() {
        let leagues = vec![league("lck", "LCK", 1)];
        assert!(find_league(&leagues, "nope").is_err());
    }

    #[test]
    fn current_tournament_prefers_the_split_containing_today() {
        let today = chrono::Local::now().date_naive();
        let running = tournament(
            "running",
            &(today - chrono::Duration::days(3)).to_string(),
            &(today + chrono::Duration::days(30)).to_string(),
        );
        let finished = tournament("finished", "2020-01-01", "2020-03-01");
        let picked = current_tournament(&[finished, running]).unwrap();
        assert_eq!(picked.slug, "running");
    }

    #[test]
    fn current_tournament_falls_back_to_the_latest_past_split() {
        let older = tournament("older", "2019-01-01", "2019-03-01");
        let newer = tournament("newer", "2020-01-01", "2020-03-01");
        let picked = current_tournament(&[newer, older]).unwrap();
        assert_eq!(picked.slug, "newer");
        assert!(current_tournament(&[]).is_none());
    }
}
