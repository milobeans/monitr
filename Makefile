.PHONY: build check fmt install-local run

build:
	cargo build --release

check:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test
	cargo build --release

fmt:
	cargo fmt

install-local:
	cargo build --release
	mkdir -p "$$HOME/.local/bin"
	install -m 0755 target/release/monitr "$$HOME/.local/bin/monitr"

run:
	cargo run -- --interval 1000
