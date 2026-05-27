# Convenience targets — see scripts/README.md for details.
.PHONY: bootstrap test fmt clippy preflight-lite preflight install-vectorc

bootstrap:
	bash scripts/bootstrap-dev.sh

test:
	cargo test --workspace --locked

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets --locked -- -D warnings

preflight-lite:
	bash scripts/preflight-lite.sh

preflight:
	bash scripts/preflight.sh

install-vectorc:
	cargo install --path crates/vc-cli --locked
