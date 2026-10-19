# Developing on macOS

## Setup

    brew install rustup
    rustup default stable

No Xcode project is involved. Local builds are signed with the first "Apple Development"
certificate in your keychain. To use a different one, put `CODESIGN_IDENTITY=<sha1 or name>`
in `.env`.

## Two separate apps

Debug and release builds have different bundle ids, so macOS treats them as two unrelated apps.

|                    | Dev build                                  | Release build                          |
|--------------------|--------------------------------------------|----------------------------------------|
| App                | `~/Applications/Browsers Dev.app`          | `/Applications/Browsers.app`           |
| Bundle id          | `software.Browsers.dev`                    | `software.Browsers`                    |
| Config             | `~/Library/Application Support/software.Browsers.dev/` | `~/Library/Application Support/software.Browsers/` |
| Cache              | `~/Library/Caches/software.Browsers.dev/`  | `~/Library/Caches/software.Browsers/`  |
| Logs               | `~/Library/Logs/software.Browsers.dev/`    | `~/Library/Logs/software.Browsers/`    |
| App repository     | read live from `resources/` in this repo   | bundled in the .app                    |

Experimenting in the dev build never touches the settings of the copy you use every day.

## Commands

| Command                | What it does                                                        |
|------------------------|---------------------------------------------------------------------|
| `make run`             | `cargo run` with a test URL (`make run URL=https://...`)            |
| `make run-settings`    | open the Settings window directly                                   |
| `make dev`             | build + install `Browsers Dev.app`, open a test URL with it         |
| `make install`         | build + install your own release build to `/Applications`           |
| `make default-dev`     | make `Browsers Dev` the default browser (to test real link clicks)  |
| `make default-release` | switch the default browser back to `Browsers`                       |
| `make logs`            | tail the dev build's log                                            |
| `make test` / `check`  | tests / fmt + clippy                                                |
| `make clean-dev`       | remove the dev app and all of its data                              |

Most changes only need `make run`. Use `make dev` + `make default-dev` when you need macOS to
actually route clicked links to your build, then `make default-release` when done.

## Native popup (macOS)

The link picker on macOS 13+ is SwiftUI, in `native/macos/BrowsersPopup/` (see `CONTRACT.md` there
for how it talks to the Rust side in `src/macos/native_popup.rs`).
`build.rs` compiles every `.swift` file in that folder with `swiftc` from the Command Line Tools,
so there's still no Xcode project.

The Slint popup (`ui/main_window.slint`) is still used on older macOS, with the "Native popup"
setting off, with `SLINT_BACKEND=headless` (the MCP test mode in CLAUDE.md), or when forced:

    BROWSERS_POPUP=slint cargo run -- https://example.com

Settings and About are native windows too on macOS 13+ (`SETTINGS_CONTRACT.md` and
`src/macos/native_settings.rs`), whatever the "Native popup" setting says.
Both versions show the same model from `src/gui/settings_model.rs` and call the same handlers in
`src/gui/settings_window.rs`, so a change there reaches both.
`BROWSERS_UI=slint` forces the Slint popup, Settings and About (`BROWSERS_POPUP=slint` does too).

## Remotes

- `origin` - your fork (Focus2321/browsers)
- `upstream` - Browsers-software/browsers

Sync with upstream: `git fetch upstream && git merge upstream/main`

## Isolated data folder (debug builds)

Set `BROWSERS_DATA_DIR` to give a debug build its own config, cache, logs and runtime folders:

    BROWSERS_DATA_DIR=/tmp/browsers-test cargo run -- https://example.com

Release builds ignore it.
