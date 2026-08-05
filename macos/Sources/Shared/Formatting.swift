//
//  Formatting.swift
//  Small display helpers shared by the app and the widget.
//

import Foundation

enum Format {
    /// "Thu 19:00" in the viewer's timezone.
    static func kickoff(_ date: Date) -> String {
        date.formatted(.dateTime.weekday(.abbreviated).hour().minute())
    }

    /// "Thu 6 Aug" — used where the day matters more than the time.
    static func day(_ date: Date) -> String {
        date.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated))
    }

    /// A one-line subtitle for a match: "Bo3 · Week 11".
    static func detail(_ match: MatchInfo) -> String {
        [match.format, match.block.isEmpty ? nil : match.block]
            .compactMap { $0 }
            .joined(separator: " · ")
    }
}
