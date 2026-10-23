# The local dev gate — one command for what CI enforces.
#
# These recipes mirror .github/workflows/ci.yml so you can run the whole gate
# before pushing: `just gate`. Std tooling only (cargo + rustfmt/clippy + typos);
# install `just` with `cargo install just`. Keep this in sync with ci.yml.
#
# Linux note: the build needs fontconfig headers — `sudo apt-get install -y
# libfontconfig1-dev` (the CI runners do the same).

# List the recipes.
default:
    @just --list

# Fast type-check across the workspace (ci.yml: test job, step 1).
check:
    cargo check --workspace --all-targets --all-features

# Headless test suites (ci.yml: test job, step 2).
test:
    cargo test --workspace --all-features

# Formatting — the blocking `--check` CI runs. `just fmt-fix` to apply.
fmt:
    cargo fmt --all --check

# Apply formatting in place (not a gate; convenience).
fmt-fix:
    cargo fmt --all

# Lints (ci.yml: clippy job — denies warnings).
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Rustdoc with the same broken-link denies CI enforces (ci.yml: docs job).
doc:
    RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links" \
        cargo doc --workspace --no-deps --all-features

# Spell-check (ci.yml: typos job). Skipped with a note if `typos` isn't installed
# (`cargo install typos-cli`).
typos:
    @if command -v typos >/dev/null 2>&1; then typos; else echo "typos not installed — skipping (cargo install typos-cli)"; fi

# The full gate — everything CI would block a PR on, in one run.
gate: fmt clippy check test doc typos wasm

# Web build check (ci.yml: wasm). The umbrella's default `webgl` feature keeps the WebGL2 fallback.
wasm:
    cargo check -p pebbles-shell -p pebbles --target wasm32-unknown-unknown

# GPU tests (ci.yml: gpu-wgpu-linux). Prefix with the LAVAPIPE env on a GPU-less box.
gpu:
    cargo test -p pebbles-gpu -- --ignored --test-threads=1
    cargo test -p pebbles-shell -- --ignored --test-threads=1

# Render check (05 §6). Needs a GPU, or prefix with the LAVAPIPE env (00 §7).
render-check:
    cargo run -p render-check --release -- --out target/render-check --determinism

# List the render corpus (no GPU).
render-check-list:
    cargo run -p render-check --release -- --list

# Render check against the committed self-goldens (after V12/V18).
render-check-refs:
    cargo run -p render-check --release -- --out target/render-check --reference dir --determinism
