//
//  WidgetViews.swift
//  Small / medium / large layouts.
//
//  Small  — identity, standing, and the one match that matters next.
//  Medium — that, plus recent form.
//  Large  — that, plus the full upcoming and recent lists.
//

import SwiftUI
import WidgetKit

struct TeamEntryView: View {
    @Environment(\.widgetFamily) private var family
    let entry: TeamEntry

    var body: some View {
        if let snapshot = entry.snapshot {
            switch family {
            case .systemSmall: SmallView(snapshot: snapshot)
            case .systemLarge: LargeView(snapshot: snapshot)
            default: MediumView(snapshot: snapshot)
            }
        } else if let message = entry.errorMessage {
            MessageView(title: "Couldn't load", message: message, isError: true)
        } else {
            MessageView(title: "Loading…", message: "Fetching the latest results.", isError: false)
        }
    }
}

// MARK: - Families

private struct SmallView: View {
    let snapshot: Snapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            TeamHeader(snapshot: snapshot, compact: true)
            Spacer(minLength: 0)
            if let match = snapshot.headline {
                MatchSummary(match: match, compact: true)
            } else {
                Text("No matches scheduled")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            FormStrip(form: snapshot.form)
        }
    }
}

private struct MediumView: View {
    let snapshot: Snapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TeamHeader(snapshot: snapshot, compact: false)
            Divider()
            if let match = snapshot.headline {
                MatchSummary(match: match, compact: false)
            } else {
                Text("No matches scheduled")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            HStack(alignment: .bottom) {
                FormStrip(form: snapshot.form)
                Spacer()
                if let last = snapshot.recent.first {
                    ResultLine(match: last)
                }
            }
        }
    }
}

private struct LargeView: View {
    let snapshot: Snapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            TeamHeader(snapshot: snapshot, compact: false)
            Divider()

            if !snapshot.live.isEmpty {
                SectionLabel("Live now")
                ForEach(snapshot.live) { MatchRow(match: $0) }
            }

            SectionLabel(snapshot.live.isEmpty ? "Next" : "Then")
            if snapshot.upcoming.isEmpty {
                Text("Nothing scheduled")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(snapshot.upcoming.prefix(3)) { MatchRow(match: $0) }
            }

            SectionLabel("Recent")
            if snapshot.recent.isEmpty {
                Text("No results yet")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(snapshot.recent.prefix(4)) { MatchRow(match: $0) }
            }

            Spacer(minLength: 0)
        }
    }
}

// MARK: - Pieces

private struct TeamHeader: View {
    let snapshot: Snapshot
    let compact: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(snapshot.league.name.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)

            Text(compact ? snapshot.team.code : snapshot.team.name)
                .font(compact ? .title2.bold() : .title3.bold())
                .lineLimit(1)
                .minimumScaleFactor(0.7)

            if let standing = snapshot.standing {
                Text("#\(standing.ordinal) · \(standing.record)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            } else if let split = snapshot.split {
                Text(split)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }
}

/// The headline match — live score, or the next opponent with a countdown.
private struct MatchSummary: View {
    let match: MatchInfo
    let compact: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 4) {
                if match.isLive {
                    LivePip()
                }
                Text(match.isLive ? "\(opponentLabel) · \(match.score)" : "vs \(opponentLabel)")
                    .font(compact ? .headline : .title3.weight(.semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
            }

            if match.isLive {
                Text(Format.detail(match))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } else {
                HStack(spacing: 4) {
                    Text(Format.kickoff(match.startTime))
                    Text("·")
                    // Auto-counts down without waiting for a timeline refresh.
                    Text(match.startTime, style: .relative)
                }
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
        }
    }

    private var opponentLabel: String {
        guard let opponent = match.opponent else { return match.away.name }
        return compact ? opponent.code : opponent.name
    }
}

private struct MatchRow: View {
    let match: MatchInfo

    var body: some View {
        HStack(spacing: 8) {
            if match.isCompleted, let outcome = match.outcome {
                Text(outcome == "win" ? "W" : "L")
                    .font(.caption.bold())
                    .foregroundStyle(outcome == "win" ? Color.green : Color.red)
                    .frame(width: 12)
            } else if match.isLive {
                LivePip().frame(width: 12)
            } else {
                Text("·").font(.caption).foregroundStyle(.secondary).frame(width: 12)
            }

            Text(match.opponent?.name ?? "\(match.home.code) / \(match.away.code)")
                .font(.caption)
                .lineLimit(1)

            Spacer(minLength: 4)

            Text(match.isCompleted || match.isLive ? match.score : Format.day(match.startTime))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        }
    }
}

private struct ResultLine: View {
    let match: MatchInfo

    var body: some View {
        Text("Last: \(match.score) \(match.opponent.map { "vs \($0.code)" } ?? "")")
            .font(.caption2)
            .foregroundStyle(.secondary)
            .lineLimit(1)
    }
}

/// Recent results as pips, newest on the left.
private struct FormStrip: View {
    let form: [Bool]

    var body: some View {
        HStack(spacing: 3) {
            ForEach(Array(form.prefix(5).enumerated()), id: \.offset) { _, isWin in
                RoundedRectangle(cornerRadius: 2, style: .continuous)
                    .fill(isWin ? Color.green : Color.red)
                    .frame(width: 12, height: 4)
            }
        }
        .opacity(0.85)
    }
}

private struct LivePip: View {
    var body: some View {
        Circle()
            .fill(Color.red)
            .frame(width: 6, height: 6)
    }
}

private struct SectionLabel: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text.uppercased())
            .font(.caption2.weight(.semibold))
            .foregroundStyle(.secondary)
    }
}

private struct MessageView: View {
    let title: String
    let message: String
    let isError: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.headline)
                .foregroundStyle(isError ? Color.red : Color.primary)
            Text(message)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(4)
            Spacer(minLength: 0)
        }
    }
}
