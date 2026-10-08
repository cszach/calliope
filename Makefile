PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin
DATADIR := $(PREFIX)/share
APP_ID := io.github.cszach.Muse

.PHONY: all build run check fmt clippy test

all: build

build:
	cargo build --release

run:
	cargo run -- --debug

check: fmt clippy test

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test --all-targets
