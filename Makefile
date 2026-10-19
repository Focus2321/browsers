# Local development on macOS - see DEVELOPING.md

URL ?= https://example.com

DEV_APP := $(HOME)/Applications/Browsers Dev.app
RELEASE_APP := /Applications/Browsers.app

.PHONY: run run-settings dev open-dev install default-dev default-release logs check test fmt clean-dev

# fastest loop: run the debug binary straight from cargo (uses the dev identity)
run:
	cargo run -- "$(URL)"

run-settings:
	cargo run -- --settings

# build + install "Browsers Dev.app" to ~/Applications, then open a link with it
dev:
	./scripts/mac-local-app.sh dev
	open -a "$(DEV_APP)" "$(URL)"

open-dev:
	open -a "$(DEV_APP)" "$(URL)"

# build + install your own release build as /Applications/Browsers.app
install:
	./scripts/mac-local-app.sh release

# switch which build macOS sends links to
default-dev:
	swift scripts/set-default-browser.swift "$(DEV_APP)"

default-release:
	swift scripts/set-default-browser.swift "$(RELEASE_APP)"

logs:
	tail -f "$(HOME)/Library/Logs/software.Browsers.dev/browsers.log"

check:
	cargo fmt --check
	cargo clippy

test:
	cargo test

fmt:
	cargo fmt

# remove the dev app and all of its data (never touches the release build's data)
clean-dev:
	pkill -f "$(DEV_APP)/Contents/MacOS/Browsers" || true
	/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u "$(DEV_APP)" || true
	rm -rf "$(DEV_APP)"
	rm -rf "$(HOME)/Library/Application Support/software.Browsers.dev"
	rm -rf "$(HOME)/Library/Caches/software.Browsers.dev"
	rm -rf "$(HOME)/Library/Logs/software.Browsers.dev"
