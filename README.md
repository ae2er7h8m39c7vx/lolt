# lolt

A League of Legends esports tracker for the terminal, written in Rust.

Defaults to the **LCK** and **T1** — running `lolt` with no arguments shows T1's
league position, next fixtures and recent results.

```
$ lolt
LCK · Split 3 2026 · KOREA
T1  ·  #3 Legend Group  ·  1W-1L

NEXT
WHEN                STAGE    HOME           AWAY                 INFO
Thu 06 Aug 10:00    Week 11  Dplus KIA  vs  T1                   Bo3 · in 22h 14m
Sat 08 Aug 08:00    Week 11         T1  vs  Hanwha Life Esports  Bo3 · in 2d 20h

RECENT
WHEN                 STAGE    HOME                 AWAY         INFO
Fri 31 Jul 08:00  W  Week 10  Gen.G Esports  0 - 2  T1          Bo3
Wed 29 Jul 10:00  L  Week 10             T1  0 - 2  kt Rolster  Bo3
```

## Install

```sh
cargo build --release        # binary at target/release/lolt
cargo install --path .       # or install it onto your PATH
```

## Usage

```
lolt [OPTIONS] [COMMAND]
```

| Command | What it shows |
| --- | --- |
| *(none)* / `overview` | Standing, next matches and recent results for the tracked team |
| `schedule` (`sched`) | League fixtures; `--past` for results, `--team-only` to filter |
| `standings` (`table`) | Current split standings, tracked team highlighted |
| `team` (`roster`) | Team profile: roster by lane, recent and upcoming matches |
| `live` | Matches in progress; `--all` for every league |
| `leagues [FILTER]` | Every league the API knows, with its slug |
| `teams` | Teams in the league's current split |

### Options

| Flag | Default | Notes |
| --- | --- | --- |
| `-l, --league <NAME>` | `lck` | Slug or name: `lck`, `lpl`, `lec`, `lta_n`, `worlds`, … |
| `-t, --team <NAME>` | `t1` | Slug, code or name: `t1`, `geng`, `"Hanwha Life Esports"` |
| `--json` | off | Raw JSON instead of tables, for scripting |
| `--color <WHEN>` | `auto` | `auto` \| `always` \| `never` (honours `NO_COLOR`) |
| `--timeout <SECS>` | `20` | Per-request timeout |

Leagues and teams are matched loosely, so `--league "korea"`, `-t "gen.g"` and
`-t GEN` all resolve. Unknown input lists the valid options.

### Examples

```sh
lolt                            # T1 in the LCK
lolt -l lec -t g2 standings     # LEC table with G2 highlighted
lolt schedule --past -n 5       # last five LCK results
lolt schedule --team-only       # only T1's upcoming fixtures
lolt live --all                 # everything live right now
lolt team                       # T1's roster and form
lolt leagues emea               # leagues in a region
lolt --json standings | jq .    # machine-readable output
```

### Changing the defaults

Set them per shell instead of typing flags:

```sh
export LOLT_LEAGUE=lec
export LOLT_TEAM=g2-esports
```

## Data

Match, standings and roster data come from the public lolesports.com API
(`esports-api.lolesports.com`) — the same endpoints the official site uses.
It needs no account, and the client ships with the site's public API key;
override it with `--api-key` or `LOLESPORTS_API_KEY` if Riot rotates it.

Times are converted to your machine's local timezone. This project is not
affiliated with or endorsed by Riot Games.

## Development

```sh
cargo test      # unit tests for matching, layout and time formatting
cargo clippy --all-targets
cargo fmt
```

Layout:

| File | Role |
| --- | --- |
| [src/main.rs](src/main.rs) | CLI definition and dispatch |
| [src/api.rs](src/api.rs) | Blocking HTTP client, one method per endpoint |
| [src/models.rs](src/models.rs) | Serde types for the API payloads |
| [src/resolve.rs](src/resolve.rs) | League/team/split name resolution |
| [src/commands.rs](src/commands.rs) | One function per subcommand |
| [src/render.rs](src/render.rs) | Colours, aligned tables, time formatting |
