//
//  LoltApp.swift
//  The container app. Widgets need one, and it doubles as the place to check
//  that the Rust core is linked and fetching before you add the widget.
//

import SwiftUI
import WidgetKit

@main
struct LoltApp: App {
    var body: some Scene {
        Window("Lolt", id: "main") {
            ContentView()
                .frame(minWidth: 420, minHeight: 460)
        }
        .windowResizability(.contentSize)
    }
}

@MainActor
final class TrackerModel: ObservableObject {
    @Published var league = "lck"
    @Published var team = "t1"
    @Published var snapshot: Snapshot?
    @Published var errorMessage: String?
    @Published var isLoading = false

    func refresh() async {
        isLoading = true
        errorMessage = nil
        let (league, team) = (self.league, self.team)

        let result = await Task.detached(priority: .userInitiated) { () -> Result<Snapshot, Error> in
            do { return .success(try LoltCore.snapshot(league: league, team: team)) } catch { return .failure(error) }
        }.value

        switch result {
        case .success(let snapshot):
            self.snapshot = snapshot
        case .failure(let error):
            self.snapshot = nil
            self.errorMessage = error.localizedDescription
        }
        isLoading = false

        // Push the new selection out to any placed widgets.
        WidgetCenter.shared.reloadAllTimelines()
    }
}

struct ContentView: View {
    @StateObject private var model = TrackerModel()

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                TextField("League", text: $model.league)
                    .frame(width: 120)
                TextField("Team", text: $model.team)
                    .frame(width: 140)
                Button("Refresh") { Task { await model.refresh() } }
                    .keyboardShortcut(.defaultAction)
                    .disabled(model.isLoading)
                if model.isLoading {
                    ProgressView().controlSize(.small)
                }
            }
            .textFieldStyle(.roundedBorder)

            Divider()

            if let snapshot = model.snapshot {
                SnapshotDetail(snapshot: snapshot)
            } else if let message = model.errorMessage {
                Label(message, systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.red)
                    .font(.callout)
            } else {
                Text("Pick a league and team, then hit Refresh.")
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Text("Rust core \(LoltCore.version) · add the widget from Notification Center → Edit Widgets")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding()
        .task { await model.refresh() }
    }
}

private struct SnapshotDetail: View {
    let snapshot: Snapshot

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(snapshot.league.name) · \(snapshot.split ?? "current split")")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(snapshot.team.name).font(.title2.bold())
                    if let standing = snapshot.standing {
                        Text("#\(standing.ordinal) \(standing.section) · \(standing.record)")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                }

                if !snapshot.live.isEmpty {
                    MatchList(title: "Live now", matches: snapshot.live)
                }
                MatchList(title: "Next", matches: snapshot.upcoming)
                MatchList(title: "Recent", matches: snapshot.recent)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private struct MatchList: View {
    let title: String
    let matches: [MatchInfo]

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)

            if matches.isEmpty {
                Text("—").foregroundStyle(.secondary).font(.callout)
            } else {
                ForEach(matches) { match in
                    HStack(spacing: 8) {
                        if let outcome = match.outcome {
                            Text(outcome == "win" ? "W" : "L")
                                .font(.caption.bold())
                                .foregroundStyle(outcome == "win" ? Color.green : Color.red)
                                .frame(width: 14)
                        } else {
                            Text(match.isLive ? "•" : "·")
                                .foregroundStyle(match.isLive ? Color.red : Color.secondary)
                                .frame(width: 14)
                        }
                        Text("\(match.home.name)  \(match.score)  \(match.away.name)")
                            .font(.callout)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        Text(Format.kickoff(match.startTime))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }
}
