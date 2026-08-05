//
//  LoltWidget.swift
//  Widget bundle, configuration intent and timeline provider.
//

import AppIntents
import SwiftUI
import WidgetKit

@main
struct LoltWidgetBundle: WidgetBundle {
    var body: some Widget {
        TeamWidget()
    }
}

// MARK: - Configuration

/// Editable from the widget's own settings (right-click → Edit Widget), so one
/// widget can track T1 and another G2 without touching code.
struct TrackTeamIntent: WidgetConfigurationIntent {
    static var title: LocalizedStringResource = "Track a Team"
    static var description = IntentDescription(
        "Choose the league and team to follow. Slugs, codes and names all work — lck/lec/lpl, t1/geng/\"Gen.G Esports\"."
    )

    @Parameter(title: "League", default: "lck")
    var league: String

    @Parameter(title: "Team", default: "t1")
    var team: String
}

// MARK: - Timeline

struct TeamEntry: TimelineEntry {
    let date: Date
    let snapshot: Snapshot?
    let errorMessage: String?

    static let placeholder = TeamEntry(date: .now, snapshot: nil, errorMessage: nil)
}

struct Provider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> TeamEntry {
        .placeholder
    }

    func snapshot(for configuration: TrackTeamIntent, in context: Context) async -> TeamEntry {
        // The gallery preview should never block on the network.
        if context.isPreview { return .placeholder }
        return await load(configuration)
    }

    func timeline(for configuration: TrackTeamIntent, in context: Context) async -> Timeline<TeamEntry> {
        let entry = await load(configuration)
        return Timeline(entries: [entry], policy: .after(refreshDate(for: entry)))
    }

    /// The core blocks on network I/O, so it runs off the main actor.
    private func load(_ configuration: TrackTeamIntent) async -> TeamEntry {
        let league = configuration.league
        let team = configuration.team
        return await Task.detached(priority: .userInitiated) {
            do {
                let snapshot = try LoltCore.snapshot(league: league, team: team)
                return TeamEntry(date: .now, snapshot: snapshot, errorMessage: nil)
            } catch {
                return TeamEntry(date: .now, snapshot: nil, errorMessage: error.localizedDescription)
            }
        }.value
    }

    /// Refresh hard while a series is running, gently in the off-season.
    /// WidgetKit treats this as a request, not a guarantee.
    private func refreshDate(for entry: TeamEntry) -> Date {
        guard let snapshot = entry.snapshot else {
            return .now.addingTimeInterval(10 * 60) // retry sooner after a failure
        }
        if !snapshot.live.isEmpty {
            return .now.addingTimeInterval(5 * 60)
        }
        if let next = snapshot.upcoming.first,
           next.startTime.timeIntervalSinceNow < 60 * 60 {
            return .now.addingTimeInterval(10 * 60) // about to start
        }
        return .now.addingTimeInterval(30 * 60)
    }
}

// MARK: - Widget

struct TeamWidget: Widget {
    let kind = "LoltTeamWidget"

    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: kind, intent: TrackTeamIntent.self, provider: Provider()) { entry in
            TeamEntryView(entry: entry)
                .containerBackground(.fill.tertiary, for: .widget)
        }
        .configurationDisplayName("Team Tracker")
        .description("Your League of Legends team's standing, next match and recent results.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}
