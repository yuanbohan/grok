SHELL=/bin/bash

.PHONY: clean build check

build:
	cargo build

clean:
	cargo clean

check:
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	cargo test -- --show-output
	cargo check
