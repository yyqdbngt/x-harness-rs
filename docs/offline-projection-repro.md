# Offline projection reproducer (Windows)

This optional diagnostic binary reuses production `restored_web_event`, history,
tail projection, JSON serialization and byte counting. It does not create a Host,
provider, tool registry or network listener, and never recovers or executes Agent
work. PowerShell here is only the external diagnostic supervisor, not an Agent
tool. The baseline is release 0.2.21 / a98d050; this is not an App replacement.

## Obtain and run

Build using the **Offline projection reproducer** GitHub Actions workflow (or the
authorized remote build server). Download the matching EXE, PDB and manifest from
the trusted workflow artifact. Do not compile Rust on a machine where repository
policy prohibits it. A matching manifest hash establishes artifact consistency,
not authenticity by itself; obtain both from a trusted build.

```powershell
./scripts/run-projection-repro.ps1 -Binary D:/repro/xharness-projection-repro.exe `
  -Manifest D:/repro/manifest.json -OutputRoot D:/repro -Synthetic

./scripts/run-projection-repro.ps1 -Binary D:/repro/xharness-projection-repro.exe `
  -Manifest D:/repro/manifest.json -OutputRoot D:/repro `
  -Journal D:/private-snapshots/session-example.jsonl -Rounds 100 -Workers 1 `
  -Seconds 120 -PageHeap -Debugger D:/debuggers/amd64/cdb.exe
```

Use a stable historical copy. The source is opened read-only, copied and hashed;
changes detected while copying abort the run. An immutable `input.jsonl` is kept
separately from the disposable recovery store (JSONL recovery can truncate an
incomplete tail). No original journal is opened for writing. Empty, oversized,
invalid or lifecycle-inconsistent journals fail explicitly; nothing is repaired
in place. Existing output directories cannot be reused by the binary.

## PageHeap and first-fault capture

The supervisor copies the EXE to a **unique image name**, asks for one Windows UAC
approval and uses an elevated helper only to lease that image's IFEO GlobalFlag
0x02000000 (full PageHeap). The actual reproducer runs at the supervisor's normal
privilege level. Never run the whole supervisor as administrator unnecessarily.
The installed desktop/Host and their registry entries are not targeted.

CDB prints `!gflag` and `!heap -s`; the supervisor requires runtime evidence of
PageHeap. It stops at the first access violation, heap corruption, fail-fast or
post-startup breakpoint, saves `first-fault.dmp`, and terminates rather than
continuing a corrupted process. A breakpoint alone is not proof of a memory bug;
inspect the exception and stack. This is **PageHeap**, not a claim that every
Application Verifier test suite is enabled. Other verifier suites can be added
later if the failure points to a specific API contract.

The helper removes only its owned, previously absent image entry on completion
or lease expiry. The supervisor verifies its absence. If interrupted, create the
run directory's `pageheap.stop` marker and let the helper finish. If the helper
was itself killed, inspect that exact unique image's IFEO entry as administrator;
remove it only after verifying `ProjectionReproOwner` matches the artifact hash
and it has no unrelated values/subkeys. Do not clear IFEO globally. Reboot alone
does not clear PageHeap settings.

## Limits and interpretation

- Default: 1 worker, 100 rounds, 120-second workload deadline; input <=64 MiB.
- Supervisor permits a 30-second startup/operation margin and samples private
  memory every 250 ms (default 2 GiB ceiling). This is a sampled cutoff, **not a
  hard allocation cap**; a single operation can allocate before the next sample.
- Full PageHeap has substantial memory overhead. Start with synthetic input and
  one worker. An instrumentation memory-budget stop is not the original bug.
- Private snapshots and full dumps may contain conversation text and credentials.
  Store locally on a private disk, never commit/upload them automatically. The
  progress log contains phase/indices, not conversation payloads. No disk cleanup
  deletes evidence automatically; budget space before repeated runs.
- `supervisor-result.json` distinguishes setup, instrumentation verification,
  timeout, workload, and native-fault outcomes. `payload/result.json` alone cannot
  prove PageHeap was active. Missing output is a failure, not a passing test.
- A successful finite run means **not reproduced with this input and workload**.
  It does not establish that the Host is fixed. Isolation changes scheduling and
  heap layout and deliberately excludes models, tools and other Host subsystems.

References: [Microsoft full PageHeap flag](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/enable-page-heap),
[PageHeap scope and persistence](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/gflags-and-pageheap).
