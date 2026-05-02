.PHONY: build check fmt install-local run

build:
	cargo build --release

check:
	cargo fmt --check
	cargo clippy -- -D warnings
	cargo test
	cargo build --release

fmt:
	cargo fmt

install-local:
	cargo build --release
	mkdir -p "$$HOME/.local/bin"
	install -m 0755 target/release/rmon "$$HOME/.local/bin/rmon"

run:
	cargo run -- --interval 1000
