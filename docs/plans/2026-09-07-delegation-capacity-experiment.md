# Delegation Capacity Experiment Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Compare 2/4/8 child-turn permits without changing shipping defaults, user data, or installed applications.

**Architecture:** Exercise the existing BasicHost, durable inbox, supervisor, loop, tool executor and JSONL session store. A test-only capacity hook and scripted provider isolate scheduling costs from paid API variability. This is a scheduler experiment, not a coding-quality or production-memory benchmark.

**Tech Stack:** Rust/Tokio Host tests, Python evidence collector, Windows/Linux/macOS GitHub Actions.

---

## Design and boundaries

- Keep the production semaphore at two, queue admission at sixteen, session ownership and cancellation unchanged.
- Compare twelve child tasks at capacities 2/4/8, three repetitions per workload, each in a fresh process.
- Workloads: model wait; model/tool/model with isolated file round trips and controlled tool wait; provider bottleneck capped at two actual streams.
- Capture elapsed time, first-step queue latency, provider/tool peak concurrency, completed tasks and test-process memory/CPU where supported.
- Check queued cancellation without claiming input, slot reuse, admission rejection, and duplicate-free settlements after reopening the on-disk store.
- Do not infer provider rate limits, code quality, compiler cost, or desktop memory from these fixtures. Do not call live APIs or access user sessions/secrets.
- Remote Rust server is preferred; it currently closes SSH connections. Use previously authorized CI compilation, never local Rust compilation.

### Task 1: Isolated fixtures

**Files:** `crates/xharness-host/src/delegation_capacity_experiment.rs`, `crates/xharness-host/src/delegation.rs`, `crates/xharness-host/src/runtime.rs`, `crates/xharness-host/Cargo.toml`, `Cargo.lock`.

1. Add opt-in ignored experiment tests with strict outcome assertions.
2. Add a `cfg(test)` hook that only increases fresh runtime capacity to 2/4/8. No environment configuration in production.
3. Use the existing JSONL store as a dev dependency and isolated temporary workspaces.
4. Run `cargo fmt --all --check` locally; run compilation and Host/Agent tests in CI.
5. Commit the independent test harness change as the currently authenticated GitHub user.

### Task 2: Reproducible evidence

**Files:** `scripts/run-delegation-capacity-experiment.py`, `.github/workflows/delegation-capacity-experiment.yml`.

1. Build the Host library test binary once in CI, then execute exact ignored tests in fresh processes.
2. Rotate 2/4/8 order across repetitions, validate all JSON measurements, preserve stdout and summarize medians.
3. Bound each subprocess and the workflow; upload evidence even when a test fails. Require no secrets and no release permissions.
4. Run on Windows, Linux and macOS; run existing Host/Agent regression tests alongside the experiment.
5. Review failures before interpreting timing, and document observed results with explicit limitations.

### Task 3: Decision checkpoint

Report measurements, safety-test results and remaining live-provider/load tests. Changing the formal default, installing a build or publishing a release is outside this experiment.
