# Large History Load Reproduction Implementation Plan

> Use the Code verification workflow; execute locally without delegation. Rust compilation occurs only in CI.

**Goal:** Exercise the previously excluded 78–682 MB local histories using the real JSONL reader, without models, tools, or original-file writes.

**Architecture:** Extend the existing isolated executable with an explicit bounded input budget and load-only mode. Feature-gated reader checkpoints identify the current line and byte offset, and exist only in the offline diagnostic build. Keep original snapshots immutable; copy-only recovery behavior is unchanged.

**Tech Stack:** Rust, existing Windows CI workflow, PowerShell, CDB.

## Tasks

1. Add option tests for default 64 MiB / explicit 1–2048 MiB budgets and load-only requiring a journal. Add staging budget tests without large test fixtures.
2. Implement load-only result and feature-gated, bounded checkpoint file overwritten/flushed before each parse and before lifecycle restore. Never record content or credentials. Fail explicitly if requested checkpoints cannot be written.
3. CI compile and run unit/synthetic tests. Download the exact artifact; verify SHA and source. Do not upload private input.
4. Copy the full original journal set to an owner-protected local snapshot. Run each journal under first-chance CDB, beginning with the three previously excluded large files. Log each process exit, checkpoint, manifest and native fault separately. Do not confuse ordinary validation errors with native crashes.

## Alternatives and limits

The installed Host startup is not used initially: its durable recovery can resume pending work. A generic JSON checker would miss native Rust store behavior. Load-only intentionally excludes projection; if all loads pass, the next controlled comparison must retain multi-session cache/projection lifetime rather than conclude the defect is fixed. Instrumentation can change timing. Keep a diagnostic build with optional checkpointing for comparison.
