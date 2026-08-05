//
//  Snapshot.swift
//  Swift mirrors of the Rust view model in src/snapshot.rs.
//
//  Property names match the JSON keys exactly (Rust serialises camelCase), so
//  no CodingKeys or key strategy is needed. If you add a field on one side,
//  add it here too — the decode is strict about types, lenient about extras.
//

import Foundation

struct Snapshot: Codable, Equatable {
    let generatedAt: Date
    let league: LeagueInfo
    let team: TeamInfo
    let split: String?
    let standing: StandingLine?
    let live: [MatchInfo]
    let upcoming: [MatchInfo]
    let recent: [MatchInfo]

    /// The match to lead with: live if one is running, else the next fixture.
    var headline: MatchInfo? { live.first ?? upcoming.first }

    /// Most recent results first, as win/loss flags for a form strip.
    var form: [Bool] {
        recent.compactMap { match in
            guard let outcome = match.outcome else { return nil }
            return outcome == "win"
        }
    }
}

struct LeagueInfo: Codable, Equatable {
    let slug: String
    let name: String
    let region: String
}

struct TeamInfo: Codable, Equatable {
    let slug: String
    let name: String
    let code: String
    let image: String?
}

struct StandingLine: Codable, Equatable {
    let ordinal: Int
    let section: String
    let wins: Int
    let losses: Int

    var record: String { "\(wins)W-\(losses)L" }
}

struct TeamRecord: Codable, Equatable {
    let wins: Int
    let losses: Int
}

struct Side: Codable, Equatable {
    let name: String
    let code: String
    let score: Int
    let record: TeamRecord?
}

struct MatchInfo: Codable, Equatable, Identifiable {
    let startTime: Date
    /// `unstarted` | `inProgress` | `completed`
    let state: String
    let block: String
    let league: String
    let leagueSlug: String
    let bestOf: Int?
    let home: Side
    let away: Side
    /// `win` / `loss` from the tracked team's point of view, once decided.
    let outcome: String?
    /// The other side, when a tracked team is playing.
    let opponent: Side?
    let involvesFocus: Bool

    var id: String { "\(startTime.timeIntervalSince1970)-\(home.code)-\(away.code)" }

    var isLive: Bool { state == "inProgress" }
    var isCompleted: Bool { state == "completed" }
    var isWin: Bool { outcome == "win" }

    /// "2 - 0" once there is something to show, else "vs".
    var score: String {
        (isCompleted || isLive) ? "\(home.score) - \(away.score)" : "vs"
    }

    var format: String? { bestOf.map { "Bo\($0)" } }
}
