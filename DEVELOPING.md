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

## Remotes

- `origin` - your fork (Focus2321/browsers)
- `upstream` - Browsers-software/browsers

Sync with upstream: `git fetch upstream && git merge upstream/main`

## Isolated data folder (debug builds)

Set `BROWSERS_DATA_DIR` to give a debug build its own config, cache, logs and runtime folders:

    BROWSERS_DATA_DIR=/tmp/browsers-test cargo run -- https://example.com

Release builds ignore it.
