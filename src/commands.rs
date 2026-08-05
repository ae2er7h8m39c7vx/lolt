//! One function per subcommand. Everything user-visible is printed here.

use anyhow::{bail, Result};
use serde::Serialize;

use crate::api::Api;
use crate::models::*;
use crate::render::{self, local_time, relative, truncate, Align, Cell, Style, Table};
use crate::resolve::{current_tournament, find_league, league_teams, norm, resolve_team};

pub struct Ctx {
    pub api: Api,
    pub style: Style,
    pub json: bool,
    pub league_query: String,
    pub team_query: String,
}

impl Ctx {
    fn league(&self) -> Result<League> {
        find_league(&self.api.leagues()?, &self.league_query)
    }

    fn team(&self, league: &League) -> Result<Team> {
        resolve_team(&self.api, league, &self.team_query)
    }

    fn emit<T: Serialize>(&self, value: &T) -> Result<()> {
        println!("{}", serde_json::to_string_pretty(value)?);
        Ok(())
    }
}

// ---------------------------------------------------------------- overview

/// Default view: where the tracked team stands, and what's next for it.
pub fn overview(ctx: &Ctx) -> Result<()> {
    let s = &ctx.style;
    let league = ctx.league()?;
    let team = ctx.team(&league)?;

    let tournaments = ctx.api.tournaments(&league.id)?;
    let split = current_tournament(&tournaments);
    let standing = split
        .as_ref()
        .map(|t| ctx.api.standings(&t.id))
        .transpose()?
        .and_then(|st| find_standing(&st, &team));

    let events = fetch_events(&ctx.api, &league.id, 1)?;
    let mine: Vec<&Event> = events.iter().filter(|e| e.involves(&team)).collect();

    if ctx.json {
        #[derive(Serialize)]
        struct Out<'a> {
            league: &'a League,
            team: &'a Team,
            split: &'a Option<Tournament>,
            standing: Option<StandingLine>,
            matches: Vec<&'a Event>,
        }
        return ctx.emit(&Out {
            league: &league,
            team: &team,
            split: &split,
            standing: standing.clone(),
            matches: mine,
        });
    }

    let label = split
        .as_ref()
        .map(|t| split_label(t, &league))
        .unwrap_or_else(|| "current split".to_string());
    println!(
        "{}",
        s.dim(&format!("{} · {} · {}", league.name, label, league.region))
    );
    let record = standing
        .as_ref()
        .map(|l| format!("  ·  {}W-{}L", l.wins, l.losses))
        .unwrap_or_default();
    let rank = standing
        .as_ref()
        .map(|l| format!("  ·  #{} {}", l.ordinal, l.section))
        .unwrap_or_default();
    println!(
        "{}{}{}",
        s.focus(&team.name),
        s.bold(&rank),
        s.bold(&record)
    );

    let live: Vec<&Event> = mine.iter().copied().filter(|e| e.is_live()).collect();
    if !live.is_empty() {
        s.heading("LIVE NOW");
        print_events(ctx, &live, Some(&team));
    }

    let next: Vec<&Event> = mine
        .iter()
        .copied()
        .filter(|e| !e.is_completed() && !e.is_live())
        .take(3)
        .collect();
    s.heading("NEXT");
    if next.is_empty() {
        println!("  {}", s.dim("nothing scheduled"));
    } else {
        print_events(ctx, &next, Some(&team));
    }

    let mut recent: Vec<&Event> = mine.iter().copied().filter(|e| e.is_completed()).collect();
    recent.reverse();
    recent.truncate(5);
    s.heading("RECENT");
    if recent.is_empty() {
        println!("  {}", s.dim("no results yet"));
    } else {
        print_events(ctx, &recent, Some(&team));
    }
    println!();
    Ok(())
}

// ---------------------------------------------------------------- schedule

pub fn schedule(ctx: &Ctx, limit: usize, past: bool, team_only: bool) -> Result<()> {
    let league = ctx.league()?;
    let focus = ctx.team(&league).ok();

    let events = fetch_events(&ctx.api, &league.id, usize::from(past))?;
    let mut selected: Vec<&Event> = events
        .iter()
        .filter(|e| match &focus {
            Some(t) if team_only => e.involves(t),
            _ => true,
        })
        .collect();

    if past {
        selected.retain(|e| e.is_completed());
        selected.reverse();
    } else {
        selected.retain(|e| !e.is_completed());
    }
    selected.truncate(limit);

    if ctx.json {
        return ctx.emit(&selected);
    }

    let s = &ctx.style;
    let kind = if past { "RESULTS" } else { "UPCOMING" };
    let scope = match (&focus, team_only) {
        (Some(t), true) => format!(" · {}", t.name),
        _ => String::new(),
    };
    s.heading(&format!("{} {}{}", league.name.to_uppercase(), kind, scope));
    if selected.is_empty() {
        println!("  {}", s.dim("no matches found"));
    } else {
        print_events(ctx, &selected, focus.as_ref());
    }
    println!();
    Ok(())
}

// --------------------------------------------------------------- standings

pub fn standings(ctx: &Ctx) -> Result<()> {
    let s = &ctx.style;
    let league = ctx.league()?;
    let focus = ctx.team(&league).ok();

    let tournaments = ctx.api.tournaments(&league.id)?;
    let Some(split) = current_tournament(&tournaments) else {
        bail!("no tournaments found for {}", league.name);
    };
    let standings = ctx.api.standings(&split.id)?;

    if ctx.json {
        return ctx.emit(&standings);
    }

    println!(
        "\n{} {}",
        s.bold(&format!("{} STANDINGS", league.name.to_uppercase())),
        s.dim(&format!("· {}", split_label(&split, &league)))
    );

    let mut printed = false;
    for stage in standings.iter().flat_map(|st| &st.stages) {
        for section in &stage.sections {
            if section.rankings.is_empty() {
                continue;
            }
            printed = true;
            let title = if section.name == stage.name {
                stage.name.clone()
            } else {
                format!("{} · {}", stage.name, section.name)
            };
            s.heading(&title);

            let mut table = Table::new(
                &["#", "TEAM", "CODE", "W", "L", "WIN%"],
                &[
                    Align::Right,
                    Align::Left,
                    Align::Left,
                    Align::Right,
                    Align::Right,
                    Align::Right,
                ],
            );
            for ranking in &section.rankings {
                for team in &ranking.teams {
                    let r = team.record.unwrap_or(Record { wins: 0, losses: 0 });
                    let played = r.wins + r.losses;
                    let pct = if played > 0 {
                        format!("{:.0}%", 100.0 * r.wins as f64 / played as f64)
                    } else {
                        "-".to_string()
                    };
                    let is_focus = focus
                        .as_ref()
                        .is_some_and(|f| norm(&f.code) == norm(&team.code));
                    table.row(
                        vec![
                            ranking.ordinal.to_string(),
                            truncate(&team.name, 26),
                            team.code.clone(),
                            r.wins.to_string(),
                            r.losses.to_string(),
                            pct,
                        ],
                        is_focus,
                    );
                }
            }
            table.print(s);
        }
    }

    if !printed {
        println!("  {}", s.dim("standings not published for this split yet"));
    }
    println!();
    Ok(())
}

// -------------------------------------------------------------------- team

pub fn team(ctx: &Ctx) -> Result<()> {
    let s = &ctx.style;
    let league = ctx.league()?;
    let team = ctx.team(&league)?;

    if ctx.json {
        return ctx.emit(&team);
    }

    let home = team
        .home_league
        .as_ref()
        .map(|h| format!("{} · {}", h.name, h.region))
        .unwrap_or_else(|| league.name.clone());
    println!(
        "\n{}  {}",
        s.focus(&team.name),
        s.dim(&format!("[{}]", team.code))
    );
    println!("{}", s.dim(&home));

    if !team.players.is_empty() {
        s.heading("ROSTER");
        let mut table = Table::new(&["ROLE", "PLAYER", "NAME"], &[Align::Left; 3]);
        let mut players = team.players.clone();
        players.sort_by_key(|p| role_order(&p.role));
        for p in players {
            let full = format!("{} {}", p.first_name, p.last_name);
            table.row(
                vec![
                    if p.role.is_empty() {
                        "-".to_string()
                    } else {
                        p.role.clone()
                    },
                    p.summoner_name.trim().to_string(),
                    full.trim().to_string(),
                ],
                false,
            );
        }
        table.print(s);
    }

    let events = fetch_events(&ctx.api, &league.id, 1)?;
    let mine: Vec<&Event> = events.iter().filter(|e| e.involves(&team)).collect();

    let mut recent: Vec<&Event> = mine.iter().copied().filter(|e| e.is_completed()).collect();
    recent.reverse();
    recent.truncate(5);
    if !recent.is_empty() {
        s.heading("RECENT");
        print_events(ctx, &recent, Some(&team));
    }

    let upcoming: Vec<&Event> = mine
        .iter()
        .copied()
        .filter(|e| !e.is_completed())
        .take(5)
        .collect();
    if !upcoming.is_empty() {
        s.heading("UPCOMING");
        print_events(ctx, &upcoming, Some(&team));
    }
    println!();
    Ok(())
}

// -------------------------------------------------------------------- live

pub fn live(ctx: &Ctx, all_leagues: bool) -> Result<()> {
    let s = &ctx.style;
    let events = ctx.api.live()?;

    let (league, focus) = if all_leagues {
        (None, None)
    } else {
        let l = ctx.league()?;
        let t = ctx.team(&l).ok();
        (Some(l), t)
    };

    let selected: Vec<&Event> = events
        .iter()
        .filter(|e| e.match_info.is_some())
        .filter(|e| match &league {
            Some(l) => e
                .league
                .as_ref()
                .is_some_and(|el| norm(&el.slug) == norm(&l.slug)),
            None => true,
        })
        .collect();

    if ctx.json {
        return ctx.emit(&selected);
    }

    let scope = league
        .as_ref()
        .map(|l| l.name.to_uppercase())
        .unwrap_or_else(|| "ALL LEAGUES".to_string());
    s.heading(&format!("LIVE · {scope}"));
    if selected.is_empty() {
        println!("  {}", s.dim("nothing live right now"));
        if !all_leagues {
            println!("  {}", s.dim("try `lolt live --all` to see every league"));
        }
    } else {
        print_events(ctx, &selected, focus.as_ref());
    }
    println!();
    Ok(())
}

// ------------------------------------------------------------ list helpers

pub fn leagues(ctx: &Ctx, filter: Option<&str>) -> Result<()> {
    let s = &ctx.style;
    let mut leagues = ctx.api.leagues()?;
    leagues.sort_by(|a, b| (a.priority, &a.name).cmp(&(b.priority, &b.name)));
    if let Some(f) = filter {
        let f = norm(f);
        leagues.retain(|l| norm(&l.name).contains(&f) || norm(&l.region).contains(&f));
    }

    if ctx.json {
        return ctx.emit(&leagues);
    }

    let current = find_league(&leagues, &ctx.league_query).ok();
    s.heading("LEAGUES");
    let mut table = Table::new(&["SLUG", "NAME", "REGION"], &[Align::Left; 3]);
    for l in &leagues {
        let is_focus = current.as_ref().is_some_and(|c| c.id == l.id);
        table.row(
            vec![l.slug.clone(), l.name.clone(), l.region.clone()],
            is_focus,
        );
    }
    table.print(s);
    println!();
    Ok(())
}

pub fn teams(ctx: &Ctx) -> Result<()> {
    let s = &ctx.style;
    let league = ctx.league()?;
    let focus = ctx.team(&league).ok();
    let teams = league_teams(&ctx.api, &league)?;

    if ctx.json {
        return ctx.emit(&teams);
    }

    s.heading(&format!("{} TEAMS", league.name.to_uppercase()));
    if teams.is_empty() {
        println!("  {}", s.dim("no active split found for this league"));
        println!();
        return Ok(());
    }
    let mut table = Table::new(&["CODE", "TEAM", "SLUG"], &[Align::Left; 3]);
    for t in &teams {
        let is_focus = focus
            .as_ref()
            .is_some_and(|f| norm(&f.code) == norm(&t.code));
        table.row(
            vec![t.code.clone(), t.name.clone(), t.slug.clone()],
            is_focus,
        );
    }
    table.print(s);
    println!();
    Ok(())
}

// ----------------------------------------------------------------- shared

/// Current page of the schedule plus `older_pages` earlier pages, in
/// chronological order with duplicate matches removed.
fn fetch_events(api: &Api, league_id: &str, older_pages: usize) -> Result<Vec<Event>> {
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

fn print_events(ctx: &Ctx, events: &[&Event], focus: Option<&Team>) {
    let s = &ctx.style;
    let mut table = Table::new(
        &["WHEN", "", "STAGE", "HOME", "", "AWAY", "INFO"],
        &[
            Align::Left,
            Align::Left,
            Align::Left,
            Align::Right,
            Align::Center,
            Align::Left,
            Align::Left,
        ],
    );

    for ev in events {
        let teams = ev.teams();
        let (home, away) = (teams.first(), teams.get(1));
        let score = match (home, away) {
            (Some(h), Some(a)) if ev.is_completed() || ev.is_live() => format!(
                "{} - {}",
                h.result.as_ref().map_or(0, |r| r.game_wins),
                a.result.as_ref().map_or(0, |r| r.game_wins)
            ),
            _ => "vs".to_string(),
        };

        let mut info = Vec::new();
        if let Some(bo) = ev.best_of() {
            info.push(format!("Bo{bo}"));
        }
        if ev.is_live() {
            info.push("LIVE".to_string());
        } else if !ev.is_completed() {
            info.push(relative(ev.start_time));
        }
        if let Some(l) = &ev.league {
            if norm(&l.slug) != norm(&ctx.league_query) {
                info.push(l.name.clone());
            }
        }

        // Result marker, from the tracked team's point of view.
        let marker = match (focus, ev.is_completed()) {
            (Some(f), true) => teams
                .iter()
                .find(|t| norm(&t.code) == norm(&f.code))
                .and_then(|t| t.result.as_ref())
                .and_then(|r| r.outcome.as_deref())
                .map(|o| {
                    if o == "win" {
                        Cell::styled("W", render::WIN)
                    } else {
                        Cell::styled("L", render::LOSS)
                    }
                })
                .unwrap_or_else(|| Cell::from("")),
            _ => Cell::from(""),
        };

        let info_cell = if ev.is_live() {
            Cell::styled(info.join(" · "), render::LIVE)
        } else {
            Cell::styled(info.join(" · "), render::DIM)
        };

        table.row(
            vec![
                Cell::styled(local_time(ev.start_time), render::DIM),
                marker,
                Cell::from(ev.block_name.clone().unwrap_or_default()),
                Cell::from(home.map_or("TBD".to_string(), team_label)),
                Cell::from(score),
                Cell::from(away.map_or("TBD".to_string(), team_label)),
                info_cell,
            ],
            focus.is_some_and(|f| ev.involves(f)),
        );
    }

    if !table.is_empty() {
        table.print(s);
    }
}

fn team_label(t: &MatchTeam) -> String {
    if t.name.is_empty() {
        t.code.clone()
    } else {
        truncate(&t.name, 22)
    }
}

fn role_order(role: &str) -> u8 {
    match role.to_lowercase().as_str() {
        "top" => 0,
        "jungle" => 1,
        "mid" => 2,
        "bottom" => 3,
        "support" => 4,
        _ => 5,
    }
}

/// `lck_split_3_2026` + league `lck` -> `Split 3 2026`; the league name is
/// already on screen, so the redundant prefix is dropped.
fn split_label(tournament: &Tournament, league: &League) -> String {
    let slug = tournament
        .slug
        .strip_prefix(&format!("{}_", league.slug))
        .or_else(|| tournament.slug.strip_prefix(&format!("{}-", league.slug)))
        .unwrap_or(&tournament.slug);
    pretty_slug(slug)
}

/// `split_3_2026` -> `Split 3 2026`
fn pretty_slug(slug: &str) -> String {
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

#[derive(Clone, Serialize)]
pub struct StandingLine {
    pub ordinal: i64,
    pub section: String,
    pub wins: i64,
    pub losses: i64,
}

fn find_standing(standings: &[Standing], team: &Team) -> Option<StandingLine> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn split_label_drops_the_league_prefix() {
        let league = League {
            id: "1".into(),
            slug: "lck".into(),
            name: "LCK".into(),
            region: "KOREA".into(),
            priority: 1,
        };
        let tournament = Tournament {
            id: "2".into(),
            slug: "lck_split_3_2026".into(),
            start_date: NaiveDate::from_ymd_opt(2026, 7, 28).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
        };
        assert_eq!(split_label(&tournament, &league), "Split 3 2026");
    }

    #[test]
    fn roles_sort_into_lane_order() {
        let mut roles = ["support", "top", "bottom", "jungle", "mid", "coach"];
        roles.sort_by_key(|r| role_order(r));
        assert_eq!(
            roles,
            ["top", "jungle", "mid", "bottom", "support", "coach"]
        );
    }
}
