#!/usr/bin/env python3
"""Release gate for one commit (wgpu plan 11 G41, G44): every required CI lane has a green run ON
that commit.

    scripts/release-check.py <sha>                       # pebbles-hq/pebbles at <sha>
    scripts/release-check.py <sha> --sibling pebbles-hq/pebbles-charts=<sha2> ...
    scripts/release-check.py --self-test

For each workflow, the LATEST run on the commit (any event: push, pull_request, workflow_dispatch)
is the one that counts; its jobs are matched against REQUIRED below. Prints one line per lane
(`ok` / `RED` / `MISSING`, with the run URL) and exits 1 if any lane is missing or not green.
A sibling (`owner/repo=sha`) must have at least one run on its sha and every non-skipped job of
the latest run of each of its workflows green. Needs `gh` authenticated (GH_TOKEN works).
"""

import argparse
import json
import subprocess
import sys

# (workflow name, job name). Matrix jobs are listed per row. 11 G41 "Required lanes".
REQUIRED = [
    ("CI", "Test (ubuntu-latest)"),
    ("CI", "Test (windows-latest)"),
    ("CI", "Test (macos-latest)"),
    ("CI", "MSRV (1.90)"),
    ("CI", "Compile (wasm32-unknown-unknown)"),
    ("CI", "Clippy"),
    ("CI", "Rustfmt"),
    ("CI", "Rustdoc"),
    ("CI", "Typos"),
    ("CI", "Feature matrix (optional integrations)"),
    ("CI", "wgpu GPU tests (Linux lavapipe)"),
    ("CI", "wgpu GPU tests (Windows WARP)"),
    ("CI", "wgpu GPU tests (macOS Metal)"),
    ("CI", "wgpu GPU tests (Linux llvmpipe GL)"),
    ("CI", "wgpu validation, noop backend (ubuntu-latest)"),
    ("CI", "wgpu validation, noop backend (windows-latest)"),
    ("CI", "wgpu validation, noop backend (macos-latest)"),
    ("CI", "Wasm size (counter, WebGPU + WebGL2)"),
    ("CI", "Render check (lavapipe)"),
    ("CI", "Desktop smoke (Linux X11)"),
    ("Security", "cargo-audit (RustSec)"),
    ("Security", "cargo-deny (advisories)"),
    ("Security", "cargo-deny (bans)"),
    ("Security", "cargo-deny (licenses)"),
    ("Security", "cargo-deny (sources)"),
    ("Security", "Gitleaks"),
    ("Security", "Miri (unsafe / UB)"),
    ("Licenses", "Dependency licenses"),
]


def gh_json(args):
    out = subprocess.run(["gh", *args], capture_output=True, text=True)
    if out.returncode != 0:
        raise SystemExit(f"gh {' '.join(args)} failed: {out.stderr.strip()}")
    return json.loads(out.stdout or "null")


def latest_runs(repo, sha):
    """{workflow name: run} with the newest run per workflow on `sha`."""
    runs = gh_json(["run", "list", "-R", repo, "--commit", sha, "--limit", "200",
                    "--json", "databaseId,workflowName,status,conclusion,createdAt,url,event"])
    latest = {}
    for r in runs:
        w = r["workflowName"]
        if w not in latest or r["createdAt"] > latest[w]["createdAt"]:
            latest[w] = r
    return latest


def jobs_of(repo, run):
    data = gh_json(["run", "view", str(run["databaseId"]), "-R", repo, "--json", "jobs"])
    return data.get("jobs", [])


def evaluate(required, runs_jobs):
    """Pure: `runs_jobs` = {workflow: (run, [jobs])}. Returns [(workflow, job, state, url)] with
    state ok | RED (<conclusion or status>) | MISSING."""
    rows = []
    for wf, job in required:
        entry = runs_jobs.get(wf)
        if not entry:
            rows.append((wf, job, "MISSING (no run of this workflow on the commit)", ""))
            continue
        run, jobs = entry
        match = [j for j in jobs if j.get("name") == job]
        if not match:
            rows.append((wf, job, "MISSING (job not in the latest run)", run.get("url", "")))
            continue
        j = match[-1]
        if j.get("status") not in (None, "completed"):
            rows.append((wf, job, f"RED ({j.get('status')})", run.get("url", "")))
        elif j.get("conclusion") == "success":
            rows.append((wf, job, "ok", run.get("url", "")))
        else:
            rows.append((wf, job, f"RED ({j.get('conclusion')})", run.get("url", "")))
    return rows


def evaluate_sibling(runs_jobs):
    """Every non-skipped job of every workflow's latest run is green; at least one run exists."""
    if not runs_jobs:
        return [("*", "*", "MISSING (no run on the commit)", "")]
    rows = []
    for wf, (run, jobs) in sorted(runs_jobs.items()):
        for j in jobs:
            if j.get("conclusion") == "skipped":
                continue
            ok = j.get("status") in (None, "completed") and j.get("conclusion") == "success"
            rows.append((wf, j.get("name", "?"), "ok" if ok else f"RED ({j.get('conclusion') or j.get('status')})",
                         run.get("url", "")))
    return rows


def collect(repo, sha):
    return {wf: (run, jobs_of(repo, run)) for wf, run in latest_runs(repo, sha).items()}


def report(title, rows):
    print(f"## {title}")
    bad = 0
    for wf, job, state, url in rows:
        if state != "ok":
            bad += 1
        print(f"  {state:<10} {wf} / {job}  {url}")
    return bad


def self_test():
    run = {"url": "u", "createdAt": "1"}
    ok = {"name": "Clippy", "status": "completed", "conclusion": "success"}
    red = {"name": "Rustfmt", "status": "completed", "conclusion": "failure"}
    pending = {"name": "Typos", "status": "in_progress", "conclusion": None}
    rows = evaluate([("CI", "Clippy"), ("CI", "Rustfmt"), ("CI", "Typos"), ("CI", "Rustdoc"), ("Security", "Gitleaks")],
                    {"CI": (run, [ok, red, pending])})
    states = [r[2].split(" ")[0] for r in rows]
    assert states == ["ok", "RED", "RED", "MISSING", "MISSING"], states
    skipped = {"name": "Fuzz", "status": "completed", "conclusion": "skipped"}
    assert [r[2] for r in evaluate_sibling({"CI": (run, [ok, skipped])})] == ["ok"]
    assert evaluate_sibling({})[0][2].startswith("MISSING")
    assert evaluate_sibling({"CI": (run, [red])})[0][2].startswith("RED")
    print("self-test ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("sha", nargs="?")
    ap.add_argument("--repo", default="pebbles-hq/pebbles")
    ap.add_argument("--sibling", action="append", default=[], help="owner/repo=sha (repeatable)")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        self_test()
        return 0
    if not a.sha:
        ap.error("a commit sha is required")
    sha = subprocess.run(["git", "rev-parse", a.sha], capture_output=True, text=True).stdout.strip() or a.sha
    bad = report(f"{a.repo} @ {sha}", evaluate(REQUIRED, collect(a.repo, sha)))
    for s in a.sibling:
        repo, _, ssha = s.partition("=")
        if not ssha:
            ap.error(f"--sibling {s}: expected owner/repo=sha")
        bad += report(f"{repo} @ {ssha}", evaluate_sibling(collect(repo, ssha)))
    print(f"\n{'PASS' if bad == 0 else 'FAIL'}: {bad} lane(s) missing or not green")
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
