# Friends Windows Update Channel

The user approved a small-group distribution channel in yyqdbngt/x-harness-rs.
Use the existing Tauri updater with its mandatory signature verification and
explicit download/install confirmation. No commercial certificate purchase,
forced updates, upstream key changes, or API credential distribution.

Compared with manual ZIP replacement and a custom update server, GitHub Releases
provides the smallest reusable deployment. A fork-only tag workflow builds native
Windows NSIS installers and publishes an immutable versioned release, then points
the repository's latest release at it. Every client uses the same HTTPS latest
manifest URL and public key. The first release includes a lower-version bootstrap
installer so the real upgrade path can be exercised. Future releases build only
the new version. Never overwrite a published version or rotate the key silently.

Store the independently generated encrypted signing key outside the repository,
with a Windows-protected local password backup. Upload only to dedicated fork
Actions secrets. Public artifacts contain the public key, signatures and hashes,
never the private key/password or user model keys. This provides update integrity,
not Windows publisher identity or guaranteed SmartScreen reputation.

Validation: existing updater/signature regression tests, full native Windows Rust
tests, bundled sidecar checks, independently verified package signatures, strict
version/manifest tests, and a local installed-client upgrade attempt. Preserve
application data. Report OS/UI interaction limitations explicitly if encountered.
