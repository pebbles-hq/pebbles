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

- **Memory safety.** The reactive runtime and the native-menu bridge use
  `unsafe`. Any UB there is a real vulnerability class; Miri runs over the
  reactive runtime in CI for exactly this reason.
- **Untrusted *content*.** A widget must not crash, hang, or corrupt memory on
  hostile input — malformed text, adversarial Unicode, pathological layout
  sizes, huge documents. A denial-of-service via an infinite loop in a
  text scanner counts (this has happened and is regression-tested).
- **Supply chain.** Dependency advisories, license policy, and unpinned CI
  actions are gated by `cargo-audit`, `cargo-deny` and SHA-pinned workflows.
- **Secrets.** Gitleaks audits the full git history on every run.

Out of scope: anything requiring the attacker to already run code as the user
(they control the process either way), and the security of the *application*
built with Pebbles — input validation, authentication and data handling are the
app's responsibility.

## Hardening in CI

| Gate | Tool |
|---|---|
| Dependency vulnerabilities | `cargo-audit` (RustSec), daily |
| Supply-chain policy (advisories/bans/licenses/sources) | `cargo-deny` |
| Undefined behaviour in `unsafe` | Miri (Tree Borrows) |
| Secrets in git history | Gitleaks → Code Scanning |
| Repository security posture | OpenSSF Scorecard |
| Action pinning | every action pinned to a full commit SHA |
