# SPDX-License-Identifier: MulanPSL-2.0
MANIFEST := scripts/rustsbi-setup-cli/Cargo.toml
.PHONY: check test
check:
	cargo fmt --manifest-path $(MANIFEST) --all -- --check
	cargo clippy --locked --manifest-path $(MANIFEST) --workspace --all-targets --all-features -- -D warnings
test:
	cargo test --locked --manifest-path $(MANIFEST) --workspace --all-features
