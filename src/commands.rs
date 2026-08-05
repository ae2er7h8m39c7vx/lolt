//! One function per subcommand. Everything user-visible is printed here.
//!
//! Views render [`MatchInfo`] rather than raw API events, so the terminal and
//! the macOS widget describe a match the same way.

use anyhow::{bail, Result};
use serde::Serialize;

use crate::api::Api;
use crate::models::{League, Record, Team};
use crate::render::{self, local_time, relative, truncate, Align, Cell, Style, Table};
use crate::resolve::{current_tournament, find_league, league_teams, norm, resolve_team};
use crate::snapshot::{self, MatchInfo, Snapshot};

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
/// This is the same payload the macOS widget renders.
pub fn overview(ctx: &Ctx) -> Result<()> {
    let snap = snapshot::build(&ctx.api, &ctx.league_query, &ctx.team_query, 3, 5)?;
    if ctx.json {
        return ctx.emit(&snap);
    }
    print_snapshot(ctx, &snap);
    Ok(())
}

fn print_snapshot(ctx: &Ctx, snap: &Snapshot) {
    let s = &ctx.style;
    let label = snap.split.clone().unwrap_or_else(|| "current split".into());
    println!(
        "{}",
        s.dim(&format!(
            "{} · {} · {}",
            snap.league.name, label, snap.league.region
        ))
    );
    let rank = snap
        .standing
        .as_ref()
        .map(|l| format!("  ·  #{} {}", l.ordinal, l.section))
        .unwrap_or_default();
    let record = snap
        .standing
        .as_ref()
        .map(|l| format!("  ·  {}W-{}L", l.wins, l.losses))
        .unwrap_or_default();
    println!(
        "{}{}{}",
        s.focus(&snap.team.name),
        s.bold(&rank),
        s.bold(&record)
    );

    if !snap.live.is_empty() {
        s.heading("LIVE NOW");
        print_matches(ctx, &snap.live);
    }

    s.heading("NEXT");
    if snap.upcoming.is_empty() {
        println!("  {}", s.dim("nothing scheduled"));
    } else {
        print_matches(ctx, &snap.upcoming);
    }

    s.heading("RECENT");
    if snap.recent.is_empty() {
        println!("  {}", s.dim("no results yet"));
    } else {
        print_matches(ctx, &snap.recent);
    }
    println!();
}

// ---------------------------------------------------------------- schedule

pub fn schedule(ctx: &Ctx, limit: usize, past: bool, team_only: bool) -> Result<()> {
    let league = ctx.league()?;
    let focus = ctx.team(&league).ok();

    let events = snapshot::fetch_events(&ctx.api, &league.id, usize::from(past))?;
    let mut selected: Vec<MatchInfo> = events
        .iter()
        .filter_map(|e| MatchInfo::from_event(e, focus.as_ref()))
        .filter(|m| !team_only || m.involves_focus)
        .collect();

    if past {
        selected.retain(MatchInfo::is_completed);
        selected.reverse();
    } else {
        selected.retain(|m| !m.is_completed());
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
        print_matches(ctx, &selected);
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
        s.dim(&format!("· {}", snapshot::split_label(&split, &league)))
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

    let events = snapshot::fetch_events(&ctx.api, &league.id, 1)?;
    let mine: Vec<MatchInfo> = events
        .iter()
        .filter_map(|e| MatchInfo::from_event(e, Some(&team)))
        .filter(|m| m.involves_focus)
        .collect();

    let mut recent: Vec<MatchInfo> = mine.iter().filter(|m| m.is_completed()).cloned().collect();
    recent.reverse();
    recent.truncate(5);
    if !recent.is_empty() {
        s.heading("RECENT");
        print_matches(ctx, &recent);
    }

    let upcoming: Vec<MatchInfo> = mine
        .iter()
        .filter(|m| !m.is_completed())
        .take(5)
        .cloned()
        .collect();
    if !upcoming.is_empty() {
        s.heading("UPCOMING");
        print_matches(ctx, &upcoming);
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

    let selected: Vec<MatchInfo> = events
        .iter()
        .filter_map(|e| MatchInfo::from_event(e, focus.as_ref()))
        .filter(|m| match &league {
            Some(l) => norm(&m.league_slug) == norm(&l.slug),
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
        print_matches(ctx, &selected);
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

fn print_matches(ctx: &Ctx, matches: &[MatchInfo]) {
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

    for m in matches {
        let score = if m.is_completed() || m.is_live() {
            format!("{} - {}", m.home.score, m.away.score)
        } else {
            "vs".to_string()
        };

        let mut info = Vec::new();
        if let Some(bo) = m.best_of {
            info.push(format!("Bo{bo}"));
        }
        if m.is_live() {
            info.push("LIVE".to_string());
        } else if !m.is_completed() {
            info.push(relative(m.start_time));
        }
        if norm(&m.league_slug) != norm(&ctx.league_query) && !m.league.is_empty() {
            info.push(m.league.clone());
        }

        let marker = match m.outcome.as_deref() {
            Some("win") => Cell::styled("W", render::WIN),
            Some("loss") => Cell::styled("L", render::LOSS),
            _ => Cell::from(""),
        };
        let info_cell = Cell::styled(
            info.join(" · "),
            if m.is_live() {
                render::LIVE
            } else {
                render::DIM
            },
        );

        table.row(
            vec![
                Cell::styled(local_time(m.start_time), render::DIM),
                marker,
                Cell::from(m.block.clone()),
                Cell::from(truncate(&m.home.name, 22)),
                Cell::from(score),
                Cell::from(truncate(&m.away.name, 22)),
                info_cell,
            ],
            m.involves_focus,
        );
    }

    if !table.is_empty() {
        table.print(s);
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

#[cfg(test)]
mod tests {
    use super::*;

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
