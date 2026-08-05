# Lolt for macOS

A WidgetKit widget (plus a small container app) that shows your team's standing,
next match and recent results in Notification Center.

It does **not** reimplement the tracker. The Rust crate one directory up is
compiled to a static library and linked in, so the widget and the `lolt` CLI
share the same API client, the same team-matching rules and the same view model
(`Snapshot`). The Swift layer only decodes and draws.

```
src/ffi.rs ──> liblolt_core.a ──> lolt.h ──> LoltCore.swift ──> Widget views
```

## Requirements

- macOS 14 (Sonoma) or later — the widget uses `AppIntentConfiguration`
- Xcode 15 or later
- Rust ([rustup.rs](https://rustup.rs))
- [XcodeGen](https://github.com/yonaskolb/XcodeGen) for the generated project
  (`brew install xcodegen`), or follow the manual route below

## Build

```sh
cd macos
xcodegen generate
open Lolt.xcodeproj
```

Then press ⌘R. The Rust core builds automatically as a pre-build phase
(`scripts/build-rust.sh`) — the first build takes a minute while cargo compiles
for both architectures.

To build the library on its own:

```sh
./scripts/build-rust.sh              # universal (arm64 + x86_64)
ARCHS=arm64 ./scripts/build-rust.sh  # just this Mac, faster
```

### Adding the widget

1. Run the app once (⌘R) so macOS registers the extension. The window is also a
   quick check that the core is linked — it shows the Rust core version at the
   bottom and fetches on launch.
2. Open Notification Center → **Edit Widgets**, or right-click the desktop →
   **Edit Widgets**.
3. Search for **Lolt**, pick a size, and drop it in.
4. Right-click the placed widget → **Edit Widget** to set the league and team.

### Choosing a team

The widget's own settings take the same strings the CLI does — `lck`/`lec`/`lpl`,
`t1`/`geng`/`"Gen.G Esports"`. Defaults are `lck` and `t1`. Place the widget more
than once to track several teams.

Preview any configuration from the terminal before setting it:

```sh
cargo run -- -l lec -t g2 snapshot   # exactly the JSON the widget decodes
```

## Sizes

| Size | Shows |
| --- | --- |
| Small | League, team, standing, the next (or live) match, recent-form pips |
| Medium | The above plus the last result |
| Large | Live matches, next three fixtures, last four results |

Kick-off times use SwiftUI's `.relative` style, so the countdown ticks between
timeline refreshes rather than going stale.

## Refresh cadence

`Provider.refreshDate` asks WidgetKit for a reload every 5 minutes while a
series is live, 10 minutes just before kick-off or after a failed fetch, and 30
minutes otherwise. WidgetKit treats these as hints and applies its own budget —
a widget that hasn't been looked at in days will update less often.

## Layout

| Path | Role |
| --- | --- |
| `Sources/Shared/include/lolt.h` | C declarations, used as the bridging header |
| `Sources/Shared/LoltCore.swift` | The only code that calls into Rust |
| `Sources/Shared/Snapshot.swift` | Codable mirrors of `src/snapshot.rs` |
| `Sources/Widget/LoltWidget.swift` | Configuration intent, timeline provider |
| `Sources/Widget/WidgetViews.swift` | Small/medium/large layouts |
| `Sources/App/LoltApp.swift` | Container app |
| `scripts/build-rust.sh` | cargo + `lipo` into a universal `liblolt_core.a` |
| `project.yml` | XcodeGen spec (no `.pbxproj` checked in) |

## Manual Xcode setup

If you'd rather not use XcodeGen, `project.yml` is the specification to copy —
every setting below appears there.

1. New macOS **App** project named `Lolt` in this directory, SwiftUI lifecycle.
2. Add a **Widget Extension** target named `LoltWidget`; uncheck "Include
   Configuration Intent" (the intent is in the sources already).
3. Delete the template Swift files, then add `Sources/App` to the app target,
   `Sources/Widget` to the extension, and `Sources/Shared` to **both**.
4. For **both** targets set:
   - *Objective-C Bridging Header* → `$(SRCROOT)/Sources/Shared/include/lolt.h`
   - *Library Search Paths* → `$(SRCROOT)/../target/universal-apple-darwin/release`
   - *Other Linker Flags* → `-llolt_core -liconv -lc++ -framework Security -framework CoreFoundation -framework SystemConfiguration`
   - *User Script Sandboxing* → **No**
   - *Code Signing Entitlements* → the matching `.entitlements` file in `Sources/`
5. Add a Run Script phase to both targets, before "Compile Sources", running
   `"$SRCROOT/scripts/build-rust.sh"` with "Based on dependency analysis" off.
6. Confirm both targets have **App Sandbox → Outgoing Connections (Client)** on.

## Troubleshooting

**`ld: library 'lolt_core' not found`** — the Rust build phase hasn't run.
Execute `./scripts/build-rust.sh` once by hand and check that
`../target/universal-apple-darwin/release/liblolt_core.a` exists.

**Undefined symbols at link time** — the system library list has drifted. Ask
rustc for the authoritative one and put it in *Other Linker Flags*:

```sh
cargo rustc --release --lib -- --print native-static-libs
```

**The widget shows "Couldn't load" with a connection error** — the extension is
missing the network entitlement. Check *Signing & Capabilities* on the
`LoltWidget` target: App Sandbox with **Outgoing Connections (Client)** ticked.

**Sandbox deny errors from the build script** — set *User Script Sandboxing* to
No on both targets; cargo writes outside DerivedData.

**The widget doesn't appear in the gallery** — run the container app once, and
make sure the extension's bundle identifier is prefixed by the app's
(`com.example.lolt.app` / `com.example.lolt.app.widget`). If it appears and then
vanishes after a reboot, set a real `DEVELOPMENT_TEAM` — ad-hoc signed
extensions get unloaded.

**Stale data** — WidgetKit decides when to reload. Force one by opening the
container app and hitting Refresh, which calls
`WidgetCenter.shared.reloadAllTimelines()`.

## Note on verification

The Rust core, the C ABI and the JSON contract are tested: the FFI is exercised
by a C harness against the real API, and the Swift structs are checked
field-by-field against live payloads. The Swift/SwiftUI code itself has not been
compiled — it was written on Linux, where no Swift toolchain or Xcode exists.
Expect to fix the odd compile error on first build.

Team logos are deliberately not shown: the API serves them over plain `http`,
which App Transport Security blocks, and WidgetKit can't load images
asynchronously inside a widget anyway.
