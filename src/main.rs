//! lolt — a League of Legends esports tracker for the terminal.

mod complete;

use std::io::IsTerminal;

use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::engine::ArgValueCompleter;
use clap_complete::CompleteEnv;

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum ColorMode {
    /// Colour only when stdout is a terminal
    Auto,
    Always,
    Never,
}

use lolt_core::api::{Api, DEFAULT_API_KEY};
use lolt_core::commands::{self, Ctx};
use lolt_core::render::Style;
use lolt_core::snapshot;

#[derive(Parser)]
#[command(
    name = "lolt",
    version,
    about = "Track League of Legends esports from the terminal",
    long_about = "Track League of Legends esports from the terminal.\n\n\
                  Defaults to the LCK and T1; override per run with --league/--team, \
                  or permanently with LOLT_LEAGUE / LOLT_TEAM.",
    after_help = "EXAMPLES:\n  \
        lolt                          overview of T1 in the LCK\n  \
        lolt -l lec -t g2 standings   LEC table with G2 highlighted\n  \
        lolt schedule --past          latest LCK results\n  \
        lolt schedule --team-only     only the tracked team's fixtures\n  \
        lolt live --all               every match live right now\n  \
        lolt teams                    teams in the current split"
)]
struct Cli {
    /// League slug or name (lck, lpl, lec, lta_n, worlds, ...)
    #[arg(
        short,
        long,
        global = true,
        env = "LOLT_LEAGUE",
        default_value = "lck",
        add = ArgValueCompleter::new(complete::leagues)
    )]
    league: String,

    /// Team slug, code or name (t1, geng, "Hanwha Life", ...)
    #[arg(
        short,
        long,
        global = true,
        env = "LOLT_TEAM",
        default_value = "t1",
        add = ArgValueCompleter::new(complete::teams)
    )]
    team: String,

    /// Emit raw JSON instead of tables
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    json: bool,

    /// When to use ANSI colour; NO_COLOR is honoured unless set to `always`
    #[arg(long, global = true, value_enum, default_value_t = ColorMode::Auto)]
    color: ColorMode,

    /// API locale
    #[arg(long, global = true, default_value = "en-US", hide = true)]
    locale: String,

    /// Per-request timeout in seconds
    #[arg(long, global = true, default_value_t = 20)]
    timeout: u64,

    /// Override the lolesports API key
    #[arg(long, global = true, env = "LOLESPORTS_API_KEY", hide = true)]
    api_key: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Standing, next match and recent results for the tracked team (default)
    Overview,

    /// League fixtures, newest first
    #[command(alias = "sched")]
    Schedule {
        /// How many matches to show
        #[arg(short = 'n', long, default_value_t = 12)]
        limit: usize,

        /// Show finished matches instead of upcoming ones
        #[arg(short, long)]
        past: bool,

        /// Restrict to the tracked team
        #[arg(short = 'T', long)]
        team_only: bool,
    },

    /// Current split standings
    #[command(alias = "table")]
    Standings,

    /// Team profile: roster, recent results, upcoming matches
    #[command(alias = "roster")]
    Team,

    /// Matches in progress right now
    Live {
        /// Every league, not just the selected one
        #[arg(short, long)]
        all: bool,
    },

    /// List known leagues
    Leagues {
        /// Filter by name or region
        filter: Option<String>,
    },

    /// List the teams in the league's current split
    Teams,

    /// Print the JSON payload the macOS widget renders (always JSON)
    Snapshot,
}

fn main() {
    // Answers the shell's completion requests (`COMPLETE=zsh lolt`) and exits;
    // a normal run falls straight through.
    CompleteEnv::with_factory(Cli::command).complete();

    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let color = match cli.color {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
        }
    };

    let ctx = Ctx {
        api: Api::new(
            cli.api_key.as_deref().unwrap_or(DEFAULT_API_KEY),
            &cli.locale,
            cli.timeout,
        )?,
        style: Style::new(color),
        json: cli.json,
        league_query: cli.league,
        team_query: cli.team,
    };

    match cli.command.unwrap_or(Command::Overview) {
        Command::Overview => commands::overview(&ctx),
        Command::Schedule {
            limit,
            past,
            team_only,
        } => commands::schedule(&ctx, limit, past, team_only),
        Command::Standings => commands::standings(&ctx),
        Command::Team => commands::team(&ctx),
        Command::Live { all } => commands::live(&ctx, all),
        Command::Leagues { filter } => commands::leagues(&ctx, filter.as_deref()),
        Command::Teams => commands::teams(&ctx),
        Command::Snapshot => {
            let snap = snapshot::build(&ctx.api, &ctx.league_query, &ctx.team_query, 3, 5)?;
            println!("{}", serde_json::to_string_pretty(&snap)?);
            Ok(())
        }
    }
}
