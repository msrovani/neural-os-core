#!/usr/bin/env python3
"""Mechanical documentation-drift gate (OPCODE-0072 F0).

Measures the repo and compares against a marker block in AGENTS.md / ROADMAP.md:

    <!-- MEASURED: rs=N loc=M members=K -->

Metrics:
  rs      = count of `.rs` files (excluding target/, .git)
  loc     = total lines over those `.rs` files
  members = workspace members from the root Cargo.toml `[workspace] members`

Exit codes:
  0  markers present and within 5% (or marker missing -> warn + print to paste)
  1  any metric drifts > 5%

Usage:
  python tools/measure_repo.py           # check (gate)
  python tools/measure_repo.py --print   # print measured values + marker line
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKIP_PARTS = ("target", ".git")
DRIFT = 0.05  # 5%


def iter_rs_files():
    for dirpath, dirnames, filenames in os.walk(ROOT):
        # prune excluded dirs in-place
        dirnames[:] = [
            d for d in dirnames
            if d not in SKIP_PARTS and not d.startswith("target-")
        ]
        for fn in filenames:
            if fn.endswith(".rs"):
                yield os.path.join(dirpath, fn)


def count_loc(path):
    n = 0
    try:
        with open(path, "rb") as f:
            for _ in f:
                n += 1
    except OSError:
        return 0
    return n


def measure_rs():
    files = list(iter_rs_files())
    loc = sum(count_loc(p) for p in files)
    return len(files), loc


def parse_workspace_members():
    """Return the number of workspace members from the root Cargo.toml.

    Counts entries under [workspace] members = [ ... ]. If an entry is a glob
    (e.g. `crates/*`), expand it by counting matching directories that contain
    a Cargo.toml; otherwise count it as one member.
    """
    cargo = os.path.join(ROOT, "Cargo.toml")
    if not os.path.exists(cargo):
        return 0
    with open(cargo, "r", encoding="utf-8", errors="replace") as f:
        text = f.read()

    # Find the [workspace] section, then its members = [ ... ] array.
    m = re.search(r"\[workspace\](.*?)(?=\n\[|\Z)", text, re.S)
    if not m:
        return 0
    section = m.group(1)
    mm = re.search(r"members\s*=\s*\[(.*?)\]", section, re.S)
    if not mm:
        return 0
    entries = re.findall(r'"([^"]+)"', mm.group(1))

    count = 0
    for entry in entries:
        if "*" in entry or "?" in entry:
            # simple glob: expand against the filesystem (dirs with Cargo.toml)
            base = entry.split("*")[0].split("?")[0]
            base = base.rstrip("/\\")
            d = os.path.join(ROOT, base) if base else ROOT
            if os.path.isdir(d):
                for name in os.listdir(d):
                    sub = os.path.join(d, name)
                    if os.path.isdir(sub) and os.path.exists(
                        os.path.join(sub, "Cargo.toml")
                    ):
                        count += 1
        else:
            count += 1
    return count


def measure():
    rs, loc = measure_rs()
    members = parse_workspace_members()
    return {"rs": rs, "loc": loc, "members": members}


def marker_line(m):
    return "<!-- MEASURED: rs=%d loc=%d members=%d -->" % (
        m["rs"], m["loc"], m["members"])


MARKER_RE = re.compile(
    r"<!--\s*MEASURED:\s*rs=(\d+)\s+loc=(\d+)\s+members=(\d+)\s*-->")


def read_marker(path):
    if not os.path.exists(path):
        return None
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        text = f.read()
    mm = MARKER_RE.search(text)
    if not mm:
        return None
    return {"rs": int(mm.group(1)), "loc": int(mm.group(2)),
            "members": int(mm.group(3))}


def drifted(measured, claimed):
    if claimed == 0:
        return measured != 0
    return abs(measured - claimed) / float(claimed) > DRIFT


def main():
    measured = measure()
    line = marker_line(measured)

    if "--print" in sys.argv[1:]:
        print("[measure] measured: rs=%d loc=%d members=%d" % (
            measured["rs"], measured["loc"], measured["members"]))
        print("[measure] marker to paste:")
        print(line)
        return 0

    targets = [
        ("AGENTS.md", os.path.join(ROOT, "AGENTS.md")),
        ("ROADMAP.md", os.path.join(ROOT, "ROADMAP.md")),
    ]
    rc = 0
    for name, path in targets:
        claimed = read_marker(path)
        if claimed is None:
            print("[measure] %s: MARKER MISSING" % name)
            print("[measure]   measured rs=%d loc=%d members=%d" % (
                measured["rs"], measured["loc"], measured["members"]))
            print("[measure]   paste: %s" % line)
            continue
        bad = []
        for key in ("rs", "loc", "members"):
            if drifted(measured[key], claimed[key]):
                pct = (0.0 if claimed[key] == 0 else
                       (measured[key] - claimed[key]) * 100.0 / claimed[key])
                bad.append("%s claimed=%d measured=%d (%+.1f%%)" % (
                    key, claimed[key], measured[key], pct))
        if bad:
            rc = 1
            print("[measure] %s: DRIFT" % name)
            for b in bad:
                print("[measure]   " + b)
            print("[measure]   update to: %s" % line)
        else:
            print("[measure] %s: OK (rs=%d loc=%d members=%d)" % (
                name, claimed["rs"], claimed["loc"], claimed["members"]))

    return rc


if __name__ == "__main__":
    sys.exit(main())
