//
//  LoltCore.swift
//  The only place that talks to the Rust core.
//

import Foundation

enum LoltCoreError: LocalizedError {
    case allocationFailed
    case core(String)

    var errorDescription: String? {
        switch self {
        case .allocationFailed:
            return "The tracker core returned nothing."
        case .core(let message):
            return message
        }
    }
}

enum LoltCore {
    /// Version of the linked Rust core — handy for confirming the static
    /// library actually got rebuilt.
    static var version: String { String(cString: lolt_version()) }

    /// Fetch a snapshot. **Blocking network I/O** — call it off the main
    /// thread (see `Provider.load`).
    static func snapshot(league: String, team: String, timeout: UInt64 = 20) throws -> Snapshot {
        let raw: UnsafeMutablePointer<CChar>? = league.withCString { leaguePtr in
            team.withCString { teamPtr in
                lolt_snapshot_json(leaguePtr, teamPtr, timeout)
            }
        }

        guard let raw else { throw LoltCoreError.allocationFailed }
        defer { lolt_string_free(raw) }

        let data = Data(String(cString: raw).utf8)
        let decoder = JSONDecoder()
        // The core pins timestamps to whole seconds for exactly this decoder.
        decoder.dateDecodingStrategy = .iso8601

        let response = try decoder.decode(CoreResponse.self, from: data)
        guard response.ok, let snapshot = response.snapshot else {
            throw LoltCoreError.core(response.error ?? "The tracker core failed without a reason.")
        }
        return snapshot
    }

    private struct CoreResponse: Decodable {
        let ok: Bool
        let snapshot: Snapshot?
        let error: String?
    }
}
