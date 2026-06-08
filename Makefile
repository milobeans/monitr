.PHONY: build check fmt install-local run

build:
	cargo build --release --locked

check:
	cargo fmt --check
	cargo clippy --locked --all-targets --all-features -- -D warnings
	cargo test --locked
	cargo build --release --locked

fmt:
	cargo fmt

install-local:
	cargo build --release --locked
	mkdir -p "$$HOME/.local/bin"
	install -m 0755 target/release/monitr "$$HOME/.local/bin/monitr"

run:
	cargo run --locked -- --interval 1000
