#!/usr/bin/env python3
"""Run opt-in Host tests in fresh processes; never use live providers/user state.

Run this on the authorized Rust build server or CI, not on the developer PC.
The fixtures are deterministic scheduler loads, not real coding benchmarks.
"""
import argparse
import ctypes
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time


def windows_memory(process):
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
            (name, ctypes.c_size_t) for name in (
                "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage",
                "PagefileUsage", "PeakPagefileUsage", "PrivateUsage")]

    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    query = ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo
    query.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    query.restype = wintypes.BOOL
    # CPython keeps this process handle open until Popen is disposed.
    if not query(int(process._handle), ctypes.byref(counters), counters.cb):
        return {}
    return {"peak_rss_bytes": counters.PeakWorkingSetSize,
            "peak_private_commit_bytes": counters.PeakPagefileUsage}


def sample(binary, test, log):
    """One wrapper process measures exactly one test child, excluding compilation."""
    memory = {}
    started = time.monotonic()
    with Path(log).open("w", encoding="utf-8") as output:
        child = subprocess.Popen([binary, test, "--exact", "--ignored", "--nocapture"],
                                 stdout=output, stderr=subprocess.STDOUT)
        while child.poll() is None:
            if time.monotonic() - started > 120:
                child.kill()
                child.wait()
                raise RuntimeError(f"test timeout: {test}; evidence: {log}")
            if os.name == "nt":
                for key, value in windows_memory(child).items():
                    memory[key] = max(memory.get(key, 0), value)
            time.sleep(0.01)
        code = child.wait()
    if os.name != "nt":
        import resource
        usage = resource.getrusage(resource.RUSAGE_CHILDREN)
        memory["peak_rss_bytes"] = usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024)
        memory["cpu_seconds"] = usage.ru_utime + usage.ru_stime
    else:
        from ctypes import wintypes
        times = [wintypes.FILETIME() for _ in range(4)]
        query = ctypes.WinDLL("kernel32", use_last_error=True).GetProcessTimes
        query.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
        query.restype = wintypes.BOOL
        if query(int(child._handle), *(ctypes.byref(value) for value in times)):
            memory["cpu_seconds"] = sum((t.dwHighDateTime << 32) + t.dwLowDateTime for t in times[2:]) / 10_000_000
    text = Path(log).read_text(encoding="utf-8")
    if code:
        raise RuntimeError(f"test exited {code}: {text[-10000:]}")
    rows = [json.loads(line.split("CAPACITY_RESULT ", 1)[1])
            for line in text.splitlines() if "CAPACITY_RESULT " in line]
    if len(rows) != 1:
        raise RuntimeError(f"expected exactly one measurement: {text}")
    print(json.dumps({**rows[0], **memory, "process_wall_ms": (time.monotonic() - started) * 1000}))


def run(output):
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    command = ["cargo", "test", "--locked", "-p", "xharness-host", "--lib", "--no-run", "--message-format=json"]
    build = subprocess.run(command, text=True, encoding="utf-8", stdout=subprocess.PIPE, timeout=1200)
    (output / "build.jsonl").write_text(build.stdout, encoding="utf-8")
    build.check_returncode()
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
    binaries = [entry["executable"] for entry in artifacts
                if entry.get("reason") == "compiler-artifact" and entry.get("executable")
                and entry.get("target", {}).get("name") == "xharness_host"
                and entry.get("profile", {}).get("test")]
    if len(binaries) != 1:
        raise RuntimeError(f"expected one Host test binary, got {binaries}")
    rows = []
    jobs = [(n, "safety", 0) for n in (2, 4, 8)]
    for repetition, order in enumerate(((2, 4, 8), (4, 8, 2), (8, 2, 4)), 1):
        for profile in ("model_wait", "tool_wait", "provider_cap2"):
            jobs.extend((n, profile, repetition) for n in order)
    for n, profile, repetition in jobs:
        label = f"{profile}-{n}-{repetition}"
        env = dict(os.environ, XHARNESS_TEST_CAPACITY=str(n), XHARNESS_TEST_PROFILE=profile)
        method = "cancellation_and_admission" if profile == "safety" else "throughput"
        test = f"delegation::capacity_experiment::{method}"
        result = subprocess.run([sys.executable, __file__, "--sample-binary", binaries[0],
                                 "--test", test, "--log", str(output / f"{label}.log")],
                                env=env, text=True, encoding="utf-8", capture_output=True, timeout=150)
        if result.returncode:
            print(result.stderr, file=sys.stderr)
            raise RuntimeError(f"experiment failed: {label}; see {output}")
        row = json.loads(result.stdout)
        assert row["capacity"] == n
        if profile != "safety":
            assert row["profile"] == profile and row["tasks"] == 12
            assert row["settlements_after_reopen"] == 12
        row.update(repetition=repetition, os=platform.platform())
        rows.append(row)
        (output / "measurements.json").write_text(json.dumps(rows, indent=2), encoding="utf-8")
        print(json.dumps(row), flush=True)
    summary = []
    for profile in ("model_wait", "tool_wait", "provider_cap2"):
        for n in (2, 4, 8):
            group = [r for r in rows if r.get("profile") == profile and r["capacity"] == n]
            summary.append({"profile": profile, "capacity": n, "samples": len(group), **{
                "median_" + key: statistics.median(r[key] for r in group)
                for key in ("elapsed_ms", "first_step_wait_p95_ms", "peak_rss_bytes", "cpu_seconds")
                if all(key in r for r in group)}})
    (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print("SUMMARY " + json.dumps(summary), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", default="dist/delegation-capacity-evidence")
    parser.add_argument("--sample-binary")
    parser.add_argument("--test")
    parser.add_argument("--log")
    args = parser.parse_args()
    if args.sample_binary:
        sample(args.sample_binary, args.test, args.log)
    else:
        run(args.output)
