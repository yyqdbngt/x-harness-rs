# Live historical Host reproduction implementation plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Exercise the frequently crashing conversation with the user's configured real model, rather than offline projection alone.

**Architecture:** Run an exact copy of installed 0.2.21 Host under first-chance CDB capture, on an isolated state/workspace. Retain journal content and compaction, but reroot the copied header and force workspace-write/ask permissions before restore; never implicitly replay full-access work against the original workspace. Read the selected Windows credential only into child environment, no plaintext secret files or command-line keys.

**Tech Stack:** Prebuilt Windows Rust Host, PowerShell 7 supervisor, Windows Credential Manager, local CDB, native HTTP RPC.

---

## Task 1: Prepare private fixture and credential binding

Create a local-only run directory under D:/XHarness-backups; never commit snapshots,
keys, model responses or dumps. Verify binary/source hashes. Clone a stable journal,
reroot only the clone, append valid restricted permission events. Validate with the
prebuilt offline reproducer before executing live code. Retain original SHA256.
Copy only public source files required for a bounded coding/inspection task.

## Task 2: First-chance live run

Create a local supervisor script with explicit 600-second limit, resource sampling,
first-fault capture, private debug output and exact-process cleanup. Obtain only the
selected DeepSeek credential. Bind loopback with a random desktop API token. Use
production compaction settings; do not disable compaction or claim it occurred unless
events show it. Resume copied state and send a scoped follow-up prompt. Keep approvals
visible, do not automatically approve arbitrary commands or operations outside fixture.

## Task 3: Verification and handoff

Report request/response and turn/tool/compaction counts without response payloads.
Classify native fault, provider failure, approval wait, timeout or bounded completion
separately. Preserve complete first-fault dump if caught; verify original hash and
stop only test-owned processes. No installed App changes, no release or master push.
If this constrained run is negative, explain changed permission/workspace boundaries;
do not call it proof that the live Host is correct.
