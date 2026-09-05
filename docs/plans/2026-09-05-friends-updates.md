# Friends Windows Update Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Deliver a fork-owned signed Windows update channel for a small group.

**Architecture:** Reuse the existing Host/Tauri application and mandatory updater
signature verifier. A fork-only tag workflow builds/tests Windows, publishes all
verified assets as an immutable release, and advances the stable latest channel.

**Tech Stack:** GitHub Actions/Releases, Tauri 2, Rust, PowerShell, Python, Node.

---

### Task 1: Release contract
- Create `scripts/friends-release.py` and `scripts/test-friends-release.py`.
- Test plain semantic versions, increasing releases, repository isolation,
  manifest platform/URL/signature mapping, and bootstrap selection.
- Run Python tests locally; no local Rust compilation.

### Task 2: Windows CI
- Create `.github/workflows/friends-release.yml` for `friends-v*` tags.
- Reuse pinned sidecars, all Windows Rust tests and the Tauri bundler.
- Compile public key and rolling fork endpoint into each installer; sign both
  initial bootstrap/target packages, independently verify signatures, upload
  artifacts, and publish only complete releases. Commit independently.

### Task 3: Key provisioning
- Generate a new fork-only key outside the repository using the pinned Tauri CLI.
- Keep private material out of tool output and Git; restrict local access and
  protect the password with Windows DPAPI. Populate dedicated Actions secrets.
- Verify the uploaded secret names, not their values. Never replace existing keys.

### Task 4: Acceptance and handoff
- Push the implementation branch and initial release tag to the fork only.
- Wait for CI, download and independently verify the resulting artifacts.
- Install the bootstrap and verify updater detection, download and upgrade where
  available native UI tooling permits; preserve the existing state directory.
- Document installation, release steps, private-key backup and recovery limits in
  `docs/friends-updates.md`. Publish precise tested/unverified boundaries.

The user already authorized implementation of the proposed channel. Execute
locally in this isolated worktree without subagents. The referenced superpowers
execution package is unavailable; use the available Code workflow and CI fallback
already authorized for this repository.
