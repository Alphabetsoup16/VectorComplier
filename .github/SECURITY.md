# Security policy

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security-sensitive reports.

1. Open a [private security advisory](https://github.com/Alphabetsoup16/VectorComplier/security/advisories/new) on GitHub, **or**
2. File a minimal issue asking for a contact path if advisories are unavailable.

Include steps to reproduce, affected commands (`run`, `decode-z`, etc.), and impact (DoS, sandbox escape, etc.).

## Scope

In scope: `vectorc`, workspace crates, default Wasm policy, and documented CLI limits.

Out of scope: third-party hosts running your Wasm with custom imports, or vulnerabilities in Wasmtime/Cranelift fixed upstream (we track versions via `cargo audit`).

## Documentation

See [docs/SECURITY.md](../docs/SECURITY.md) for threat model, fuel limits, and execution posture.
