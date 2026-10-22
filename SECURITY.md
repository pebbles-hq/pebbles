# Security Policy

## Supported versions

Pebbles is pre-1.0 and moves on `main`. Security fixes land on `main`; there are
no maintained release branches yet.

| Version | Supported |
|---------|-----------|
| `main`  | ✅ |
| tagged pre-releases | ❌ (upgrade to `main`) |

## Reporting a vulnerability

**Do not open a public issue for a vulnerability.**

Use GitHub's private reporting — [Security → Report a
vulnerability](https://github.com/pebbles-hq/pebbles/security/advisories/new) —
which opens a private advisory only the maintainer can see.

Please include:

- the affected crate(s) and a commit SHA,
- what an attacker gains (memory unsafety, sandbox escape, data disclosure, …),
- a reproducer — ideally a failing test or a minimal `main.rs`.

Expect an acknowledgement within **7 days** and an assessment within **30
days**. This is a personal project, not a funded product; there is no bounty.

## Threat model

Pebbles is a **client-side desktop UI framework**. It renders a local
application's own UI — it is not a network service, a sandbox, or a security
boundary. What is in scope:

- **Memory safety.** The framework's only `unsafe` block is the Windows
  native-menu FFI bridge (`pebbles-shell/src/native_menu.rs`); the proc-macro
  additionally emits one `#[unsafe(no_mangle)]` for the app's native entry point. Any
  UB there is a real vulnerability class; Miri runs in CI for exactly this reason.
  (The reactive runtime contains no `unsafe`.)
- **Untrusted *content*.** A widget must not crash, hang, or corrupt memory on
  hostile input — malformed text, adversarial Unicode, pathological layout
  sizes, huge documents. A denial-of-service via an infinite loop in a
  text scanner counts (this has happened and is regression-tested).
- **Supply chain.** Dependency advisories, license policy, and unpinned CI
  actions are gated by `cargo-audit`, `cargo-deny` and SHA-pinned workflows.
- **Secrets.** Gitleaks audits the full git history on every run.
- **New surfaces.** The offscreen `capture` API caps dimensions and bytes and
  returns `Err` (never panics) on GPU failure; the `router` coerces paths to
  same-origin and runs navigation guards on web Back/Forward and deep links.
  Guards are a navigation-flow tool, **not** an auth boundary — protected data
  must be authorized server-side (see `router::add_guard`).

### Renderer & fonts

- **Fonts are trusted input in this release.** Bundled fonts, fonts registered with
  `App::font` / `fonts::register_user_font`, and fonts installed on the user's machine are parsed by
  `fontdb` (ttf-parser), shaped by `cosmic-text` (harfrust/skrifa) and rasterized by `swash`. An
  application that registers font bytes obtained at runtime from an untrusted source accepts that
  parser risk. The font pipeline is fuzzed (`fuzz/`, weekly) and mutation-tested on every CI run.
- **Text and images are untrusted content.** Text of any length or script must not crash or hang the
  renderer. Images are bounded at decode (32 MB compressed, 16 384 px per side) and again on the GPU
  (downscaled to the device texture limit and a per-image byte cap; the image cache is LRU-bounded).
- **GPU allocations are capped.** Every allocation whose size content can influence (images, layer
  targets, geometry buffers, capture) is bounded by the device limits and fixed budgets; out-of-memory
  is recovered (evict + retry) or degraded, never a crash.
- **Shaders are static.** All WGSL is compiled from files embedded at build time; no shader source is
  built at runtime.
- **GPU failures are survivable.** Device loss and surface loss rebuild the GPU state; a renderer
  validation error is a bug and fails CI.

Out of scope: anything requiring the attacker to already run code as the user —
they control the process either way, *including its environment*, so a file sink
like `PEBBLES_LOG_FILE=<path>` (which truncates an arbitrary path with the
process's own privileges) is by design, not a vulnerability. Also out of scope:
the security of the *application* built with Pebbles — input validation,
authentication, URL/SSRF trust, and data handling are the app's responsibility.

The sibling widget packages (`pebbles-markdown`, `pebbles-charts`,
`pebbles-code-editor`) follow this same policy and gate their own supply chain;
report an issue in any of them through this same advisory process.

## Hardening in CI

| Gate | Tool |
|---|---|
| Dependency vulnerabilities | `cargo-audit` (RustSec), daily |
| Supply-chain policy (advisories/bans/licenses/sources) | `cargo-deny` |
| Undefined behaviour in `unsafe` | Miri (Tree Borrows) |
| Secrets in git history | Gitleaks → Code Scanning |
| Repository security posture | OpenSSF Scorecard |
| Action pinning | every action pinned to a full commit SHA |
| Font pipeline fuzzing | cargo-fuzz (weekly) + seeded mutation tests (every CI run) |
| Renderer validation | PEBBLES_GPU_STRICT=1 in every GPU lane |
