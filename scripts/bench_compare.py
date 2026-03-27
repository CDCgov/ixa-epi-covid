#!/usr/bin/env python3
"""Compare benchmarks between the current working tree and a base git ref.

Builds sequentially for caching, runs simultaneously for fair comparison.
The base ref is built in a persistent worktree with a stable CARGO_TARGET_DIR
so repeated runs against the same base are fast.

Usage:
    uv run scripts/bench_compare.py [base_ref] [-- criterion_args...]

Examples:
    uv run scripts/bench_compare.py main           # compare working tree vs main
    uv run scripts/bench_compare.py HEAD            # compare uncommitted changes vs last commit
    uv run scripts/bench_compare.py main -- 10k     # only run the 10k benchmark
"""

import os
import re
import subprocess
import sys
import threading
from pathlib import Path

UNITS_TO_NS = {"ns": 1, "µs": 1e3, "μs": 1e3, "us": 1e3, "ms": 1e6, "s": 1e9}
TIME_RE = re.compile(
    r"time:\s+\[[\s\S]*?([\d.]+)\s+(\w+)\s+([\d.]+)\s+(\w+)\s+([\d.]+)\s+(\w+)\s*\]"
)
BENCH_RE = re.compile(r"^Benchmarking\s+([^:]+)")


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kwargs)


def git(*args: str) -> str:
    return run(["git", *args]).stdout.strip()


def parse_benchmarks(output: str) -> dict[str, str]:
    """Parse criterion output, returning {bench_name: median_time_str}."""
    results: dict[str, str] = {}
    current = None
    for line in output.splitlines():
        m = BENCH_RE.match(line)
        if m:
            current = m.group(1).strip()
            continue
        if current:
            m = TIME_RE.search(line)
            if m:
                results[current] = f"{m.group(3)} {m.group(4)}"
                current = None
    return results


def to_ns(s: str) -> float:
    val, unit = s.split()
    return float(val) * UNITS_TO_NS.get(unit, 0)


def print_summary(title: str, base: dict[str, str], current: dict[str, str]):
    print()
    print("=" * 70)
    print(f"  {title}")
    print("=" * 70)
    print()
    print(f"  {'Benchmark':<35} {'Base':>15} {'Current':>15}  Change")
    print(f"  {'---':<35} {'---':>15} {'---':>15}  ---")

    for name in base:
        b = base[name]
        c = current.get(name)
        if c:
            bns, cns = to_ns(b), to_ns(c)
            pct = ((cns - bns) / bns) * 100
            if pct < -2.0:
                marker = f"{pct:.1f}% faster ✓"
            elif pct > 2.0:
                marker = f"+{pct:.1f}% slower ✗"
            else:
                marker = f"{pct:+.1f}% ~"
        else:
            c = "?"
            marker = "?"
        print(f"  {name:<35} {b:>15} {c:>15}  {marker}")

    print()
    print("=" * 70)


def main():
    args = sys.argv[1:]

    base_ref = "HEAD"
    extra_args: list[str] = []
    if args:
        if "--" in args:
            sep = args.index("--")
            if sep > 0:
                base_ref = args[0]
            extra_args = args[sep + 1 :]
        else:
            base_ref = args[0]

    current_branch = git("rev-parse", "--abbrev-ref", "HEAD")
    base_sha = git("rev-parse", "--short", base_ref)
    repo_root = Path(git("rev-parse", "--show-toplevel"))

    # Persistent worktree and target dir — survives across runs for caching.
    # Keyed by sha so switching base refs gets a fresh worktree.
    worktree_dir = repo_root / ".bench-worktrees" / base_sha
    base_target_dir = repo_root / "target" / "bench-base"
    base_env = {**os.environ, "CARGO_TARGET_DIR": str(base_target_dir)}

    # 1. Set up or update worktree
    if worktree_dir.exists():
        print(f"==> Reusing worktree for {base_ref} ({base_sha})")
    else:
        print(f"==> Setting up worktree for {base_ref} ({base_sha})...")
        worktree_dir.parent.mkdir(parents=True, exist_ok=True)
        run(
            ["git", "worktree", "add", "--detach", str(worktree_dir), base_ref]
        )

    # Symlink input CSVs
    input_dir = worktree_dir / "input"
    input_dir.mkdir(exist_ok=True)
    for f in repo_root.glob("input/*.csv"):
        dest = input_dir / f.name
        if not dest.exists():
            dest.symlink_to(f)

    # 2. Build both sequentially (compilation is cached, no need to parallel)
    print(f"==> Building base ({base_ref})...")
    result = subprocess.run(
        ["cargo", "bench", "--bench", "infection_loop", "--no-run"],
        cwd=worktree_dir,
        env=base_env,
    )
    if result.returncode != 0:
        sys.exit(1)

    print(f"==> Building current ({current_branch})...")
    result = subprocess.run(
        ["cargo", "bench", "--bench", "infection_loop", "--no-run"],
        cwd=repo_root,
    )
    if result.returncode != 0:
        sys.exit(1)

    # 3. Run both benchmarks simultaneously
    print()
    print("==> Running benchmarks simultaneously...")
    print(f"    base:    {base_ref} ({base_sha})")
    print(f"    current: {current_branch}")
    print()

    bench_cmd = ["cargo", "bench", "--bench", "infection_loop", "--"]

    def stream_output(proc, prefix: str) -> str:
        lines = []
        for line in proc.stdout:
            lines.append(line)
            print(f"  [{prefix}] {line}", end="", flush=True)
        proc.wait()
        return "".join(lines)

    proc_base = subprocess.Popen(
        [*bench_cmd, "--save-baseline", "base", *extra_args],
        cwd=worktree_dir,
        env=base_env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    proc_current = subprocess.Popen(
        [*bench_cmd, "--save-baseline", "current", *extra_args],
        cwd=repo_root,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )

    base_output_container: list[str] = []
    current_output_container: list[str] = []

    def run_stream(proc, prefix, container):
        container.append(stream_output(proc, prefix))

    t_base = threading.Thread(
        target=run_stream, args=(proc_base, "base", base_output_container)
    )
    t_current = threading.Thread(
        target=run_stream,
        args=(proc_current, "current", current_output_container),
    )
    t_base.start()
    t_current.start()
    t_base.join()
    t_current.join()

    base_output = base_output_container[0]
    current_output = current_output_container[0]

    if proc_base.returncode != 0:
        print("Base benchmark failed.", file=sys.stderr)
        sys.exit(1)
    if proc_current.returncode != 0:
        print("Current benchmark failed.", file=sys.stderr)
        sys.exit(1)

    # 4. Parse and compare
    base_results = parse_benchmarks(base_output)
    current_results = parse_benchmarks(current_output)

    if not base_results:
        print("No benchmark results parsed from base run.", file=sys.stderr)
        print("Raw output:", file=sys.stderr)
        print(base_output, file=sys.stderr)
        sys.exit(1)

    title = f"{base_ref} ({base_sha}) vs {current_branch}"
    print_summary(title, base_results, current_results)


if __name__ == "__main__":
    main()
