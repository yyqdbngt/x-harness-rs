# Offline Projection Reproducer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reproduce projection memory faults from private journal copies without starting a Host, provider, terminal, or tool executor.

**Architecture:** A feature-gated standalone binary calls the existing production projection functions at release a98d050. It loads only a newly staged journal copy, records metadata-only checkpoints, and exercises per-event serialization and bounded tail/history projection. A Windows wrapper assigns a unique executable name and enables per-image full PageHeap only for that executable; settings are restored in finally and explicit recovery is documented.

**Tech Stack:** Rust, serde_json, existing JSONL store, PowerShell, Windows full PageHeap, CDB, GitHub Actions.

---

The user selected implementation of both the isolated runner and native memory detection. No further product design choice is required. Alternative approaches: the existing ignored library test is reusable but has no configurable journal/progress/limit contract; launching the actual Host risks restoring Agent work. Prefer a standalone optional binary sharing production functions. This changes no production projection behavior.

### Task 1: Reproducer and tests

Files: crates/xharness-host/Cargo.toml, src/lib.rs, src/restore.rs,
src/projection_repro.rs, src/bin/xharness-projection-repro.rs.

Implement validated CLI limits, explicit synthetic/journal input, exclusive new output directory, bounded copy with SHA256, generic errors, metadata-only progress, event round trips, exact byte-count comparisons, and repeated tail/history projection. Use one worker by default, at most four. No providers, tools, network listeners, or Host initialization. Preserve all evidence. Test invalid options/IDs, unchanged source bytes, synthetic request-header and usage events, and successful projection counts.

### Task 2: Instrumentation wrapper

Files: scripts/run-projection-repro.ps1, scripts/test-projection-repro.ps1.

Validate binary hash and unique image name, refuse existing IFEO keys, require admin only for enabling the unique per-image GlobalFlag 0x02000000, enforce a bounded run and sampled memory ceiling, save native output/dump under local evidence, and restore only owned settings. Never target xharness-host.exe or change machine-wide flags. A dry/normal mode needs no elevation. Verify PageHeap separately from a passing workload and distinguish timeout, memory stop, and crash. Test refusal/cleanup with isolated fixtures.

### Task 3: Remote build and local smoke

Files: .github/workflows/offline-projection-repro.yml, docs/offline-projection-repro.md.

Rust compilation/test happens on WZU_Server or Windows CI, never local Rust compilation. CI uses only synthetic fixtures and archives the matching EXE/PDB/hash manifest, not user data. Download the artifact, check hash, stage private journal copies locally, run a bounded baseline, then an explicitly scoped full-PageHeap run if UAC is approved. Document any blocked elevation instead of claiming instrumentation was enabled.

### Acceptance

- Production installation/config/data unchanged; no history commands replayed.
- Real projection functions used; inputs and failures never print raw user content.
- Native capture can stop at first fault and remains local.
- No reproduction is reported as a negative bounded result, not a crash fix.
- No broad registry or process cleanup; standalone Application Verifier suite is not claimed when only PageHeap is configured.
