//! Guards the boundary between the Rust core and the macOS widget.
//!
//! The widget decodes [`Snapshot`] with a strict `JSONDecoder`, and nothing in
//! the Rust build knows that `macos/Sources/Shared/Snapshot.swift` exists. This
//! test reads those Swift structs and checks them against real serialised
//! payloads, so renaming a field on one side fails here instead of showing
//! "Couldn't load" in Notification Center.
//!
//! It performs no network I/O — the payloads are built in-process.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use serde_json::Value;

use lolt_core::models::Record;
use lolt_core::snapshot::{LeagueInfo, MatchInfo, Side, Snapshot, StandingLine, TeamInfo};

/// `struct name -> [(field, swift type)]`, declaration order preserved.
type Structs = BTreeMap<String, Vec<(String, String)>>;

fn swift_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("macos/Sources/Shared/Snapshot.swift")
}

/// Pulls `let <name>: <Type>` out of each `struct` block. Computed `var`
/// properties are helpers, not part of the decoded shape, so they're skipped.
fn parse_swift_structs(source: &str) -> Structs {
    let mut structs = Structs::new();
    let mut current: Option<String> = None;

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("struct ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            structs.entry(name.clone()).or_default();
            current = Some(name);
            continue;
        }
        if trimmed == "}" {
            current = None;
            continue;
        }

        let Some(name) = current.as_ref() else {
            continue;
        };
        let Some(rest) = trimmed.strip_prefix("let ") else {
            continue;
        };
        let Some((field, ty)) = rest.split_once(':') else {
            continue;
        };
        let ty = ty.trim();
        // A trailing `{` would mean a computed property, not stored data.
        if ty.contains('{') || ty.is_empty() {
            continue;
        }
        structs
            .get_mut(name)
            .expect("struct registered above")
            .push((field.trim().to_string(), ty.to_string()));
    }
    structs
}

/// What `JSONDecoder.DateDecodingStrategy.iso8601` accepts: RFC 3339 with no
/// fractional seconds.
fn is_iso8601(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && bytes
            .iter()
            .enumerate()
            .filter(|(i, _)| ![4, 7, 10, 13, 16, 19].contains(i))
            .all(|(_, c)| c.is_ascii_digit())
}

fn check(
    structs: &Structs,
    struct_name: &str,
    value: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    let Some(fields) = structs.get(struct_name) else {
        errors.push(format!("{path}: Swift has no struct named {struct_name}"));
        return;
    };
    let Some(object) = value.as_object() else {
        errors.push(format!("{path}: expected a JSON object for {struct_name}"));
        return;
    };

    for (field, ty) in fields {
        let here = format!("{path}.{field}");
        let optional = ty.ends_with('?');
        let bare = ty.trim_end_matches('?');

        let Some(actual) = object.get(field) else {
            errors.push(format!(
                "{here}: declared in Swift but missing from the JSON"
            ));
            continue;
        };

        if actual.is_null() {
            if !optional {
                errors.push(format!(
                    "{here}: null, but Swift declares non-optional {ty}"
                ));
            }
            continue;
        }

        if let Some(element) = bare.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            match actual.as_array() {
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        check(structs, element, item, &format!("{here}[{i}]"), errors);
                    }
                }
                None => errors.push(format!("{here}: expected an array")),
            }
            continue;
        }

        match bare {
            "String" => {
                if !actual.is_string() {
                    errors.push(format!("{here}: expected String, got {actual}"));
                }
            }
            "Int" => {
                if !actual.is_i64() && !actual.is_u64() {
                    errors.push(format!("{here}: expected Int, got {actual}"));
                }
            }
            "Bool" => {
                if !actual.is_boolean() {
                    errors.push(format!("{here}: expected Bool, got {actual}"));
                }
            }
            "Date" => match actual.as_str() {
                Some(s) if is_iso8601(s) => {}
                Some(s) => errors.push(format!(
                    "{here}: \"{s}\" is not decodable by JSONDecoder .iso8601"
                )),
                None => errors.push(format!("{here}: expected a date string, got {actual}")),
            },
            nested => check(structs, nested, actual, &here, errors),
        }
    }

    // The decoder ignores unknown keys, but a key with no Swift field is
    // usually a half-finished change worth catching.
    for key in object.keys() {
        if !fields.iter().any(|(field, _)| field == key) {
            errors.push(format!(
                "{path}.{key}: present in the JSON but not in Swift {struct_name}"
            ));
        }
    }
}

fn assert_matches_swift(snapshot: &Snapshot, case: &str) {
    let source = fs::read_to_string(swift_path()).expect("macos/Sources/Shared/Snapshot.swift");
    let structs = parse_swift_structs(&source);
    assert!(
        structs.contains_key("Snapshot"),
        "no Swift structs parsed — has Snapshot.swift moved or changed shape?"
    );

    let json: Value = serde_json::to_value(snapshot).expect("snapshots serialise");
    let mut errors = Vec::new();
    check(&structs, "Snapshot", &json, "snapshot", &mut errors);

    assert!(
        errors.is_empty(),
        "the Swift widget could not decode the {case} payload:\n  {}",
        errors.join("\n  ")
    );
}

fn side(name: &str, code: &str, score: i64) -> Side {
    Side {
        name: name.to_string(),
        code: code.to_string(),
        score,
        record: Some(Record { wins: 2, losses: 1 }),
    }
}

fn match_info(state: &str, outcome: Option<&str>) -> MatchInfo {
    MatchInfo {
        start_time: Utc.with_ymd_and_hms(2026, 8, 6, 10, 0, 0).unwrap(),
        state: state.to_string(),
        block: "Week 11".to_string(),
        league: "LCK".to_string(),
        league_slug: "lck".to_string(),
        best_of: Some(3),
        home: side("Dplus KIA", "DK", 1),
        away: side("T1", "T1", 2),
        outcome: outcome.map(str::to_string),
        opponent: Some(side("Dplus KIA", "DK", 1)),
        involves_focus: true,
    }
}

fn full_snapshot() -> Snapshot {
    Snapshot {
        generated_at: Utc.with_ymd_and_hms(2026, 8, 5, 12, 0, 0).unwrap(),
        league: LeagueInfo {
            slug: "lck".to_string(),
            name: "LCK".to_string(),
            region: "KOREA".to_string(),
        },
        team: TeamInfo {
            slug: "t1".to_string(),
            name: "T1".to_string(),
            code: "T1".to_string(),
            image: Some("http://static.lolesports.com/teams/t1.png".to_string()),
        },
        split: Some("Split 3 2026".to_string()),
        standing: Some(StandingLine {
            ordinal: 3,
            section: "Legend Group".to_string(),
            wins: 1,
            losses: 1,
        }),
        live: vec![match_info("inProgress", None)],
        upcoming: vec![match_info("unstarted", None)],
        recent: vec![
            match_info("completed", Some("win")),
            match_info("completed", Some("loss")),
        ],
    }
}

/// Off-season shape: no split, no standing, nothing scheduled. Every optional
/// serialises as `null`, which the Swift optionals must tolerate.
fn empty_snapshot() -> Snapshot {
    Snapshot {
        generated_at: Utc.with_ymd_and_hms(2026, 8, 5, 12, 0, 0).unwrap(),
        league: LeagueInfo {
            slug: "worlds".to_string(),
            name: "Worlds".to_string(),
            region: "INTERNATIONAL".to_string(),
        },
        team: TeamInfo {
            slug: "t1".to_string(),
            name: "T1".to_string(),
            code: "T1".to_string(),
            image: None,
        },
        split: None,
        standing: None,
        live: vec![],
        upcoming: vec![],
        recent: vec![],
    }
}

#[test]
fn swift_decodes_a_populated_snapshot() {
    assert_matches_swift(&full_snapshot(), "populated");
}

#[test]
fn swift_decodes_a_snapshot_with_every_optional_absent() {
    assert_matches_swift(&empty_snapshot(), "off-season");
}

#[test]
fn timestamps_stay_at_whole_second_precision() {
    // Utc::now() carries nanoseconds; the serialiser must drop them or Swift's
    // .iso8601 strategy rejects the payload at runtime.
    let mut snapshot = full_snapshot();
    snapshot.generated_at = Utc::now();
    let json = serde_json::to_value(&snapshot).unwrap();

    let generated = json["generatedAt"].as_str().unwrap();
    assert!(
        is_iso8601(generated),
        "generatedAt = {generated:?}, which JSONDecoder .iso8601 cannot parse"
    );
}

#[test]
fn the_swift_side_declares_every_struct_the_payload_uses() {
    let source = fs::read_to_string(swift_path()).unwrap();
    let structs = parse_swift_structs(&source);
    for name in [
        "Snapshot",
        "LeagueInfo",
        "TeamInfo",
        "StandingLine",
        "TeamRecord",
        "Side",
        "MatchInfo",
    ] {
        let fields = structs
            .get(name)
            .unwrap_or_else(|| panic!("Swift struct {name} is missing"));
        assert!(
            !fields.is_empty(),
            "Swift struct {name} has no stored fields"
        );
    }
}
