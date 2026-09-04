# Settings Applies Compatibility Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restore the upstream settings UI by emitting only the `live` and `restart` apply modes accepted by the bundled client.

**Architecture:** Keep the upstream Web bundle unchanged and repair the Rust Host's wire contract. The Host-only `ui-onboarding` namespace already takes effect immediately after a successful settings mutation, so its public apply mode is `live`; `xharness` remains `restart`, and `permission` remains `live`.

**Tech Stack:** Rust, JSON RPC settings projection, upstream DeepSeek Harness Web bundle, GitHub Actions.

---

## Design decision

The bundled settings client validates every namespace before publishing the shared settings mirror. One invalid namespace therefore disables unrelated surfaces such as model configuration and the default permission selector. Returning `immediate` for `ui-onboarding` currently violates the client union of `live | restart` and poisons the complete response.

Changing `ui-onboarding.applies` to `live` is the smallest compatible correction. Removing the namespace would lose durable onboarding state, while widening the prebuilt client would create an unnecessary fork from upstream. The session composer model and permission selectors already use their dedicated RPCs and remain operational; this fix restores the settings pages that depend on `settings.describe`.

### Task 1: Freeze the settings wire contract

**Files:**
- Modify: `crates/xharness-host/tests/basic_host.rs`

1. Extend the existing settings RPC test to assert that `ui-onboarding` reports `applies == "live"`.
2. Assert that every described namespace uses only `live` or `restart`.
3. Preserve the existing onboarding mutation assertion so persistence behavior remains covered.

### Task 2: Emit the compatible apply mode

**Files:**
- Modify: `crates/xharness-host/src/state.rs`

1. Change only `ui-onboarding.applies` from `immediate` to `live`.
2. Run `cargo fmt --check --all` locally; do not compile Rust locally under repository policy.
3. Push the atomic fix to the existing PR branch and use GitHub Actions for workspace check, test, Clippy, and Windows desktop packaging.
4. Rebuild and launch the Windows client artifact, then verify Settings > Models and Settings > General > Permissions no longer show the schema error.

