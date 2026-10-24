#!/usr/bin/env python3
"""Run a command for at most N seconds and save its merged stdout/stderr (10 §10.4, M10).

Portable stand-in for coreutils `timeout` (missing on macOS runners). The exit status is 0 when
the command was stopped by the timeout (the §10.4 window protocols end that way), otherwise the
command's own status.

    python3 scripts/run-for.py 20 out.log -- cargo run --release -p desktop
"""

import subprocess
import sys


def main() -> int:
    if len(sys.argv) < 5 or sys.argv[3] != "--":
        print(__doc__, file=sys.stderr)
        return 2
    secs, out, cmd = float(sys.argv[1]), sys.argv[2], sys.argv[4:]
    with open(out, "wb") as f:
        p = subprocess.Popen(cmd, stdout=f, stderr=subprocess.STDOUT)
        try:
            return p.wait(timeout=secs)
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait()
            return 0


if __name__ == "__main__":
    sys.exit(main())
