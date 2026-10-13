# Software architecture fixture

This public fixture uses invented data to illustrate one architecture rule:
the domain must not depend directly on an external adapter.

The intended story is ordered:

1. Start from `bundle/`, which contains a domain, a port, and an adapter.
2. The `proposals/contradictory/delta.json` proposal adds a direct
   domain-to-adapter dependency and violates `invariants/architecture.sx`.
3. Starting from that contradictory candidate, the
   `proposals/valid/delta.json` proposal removes the direct edge and adds
   domain-to-port and port-to-adapter edges.

The correction is supplied explicitly by this fixture. Felix evaluates a
proposal; it does not generate the correction. The valid delta is a patch for
the candidate after the contradictory delta, not a patch against the original
base bundle.

The CLI can validate and check the base bundle:

```powershell
cargo run -p universo-cli -- validate examples/software-architecture/bundle --manifiesto examples/software-architecture/manifest.toml
cargo run -p universo-cli -- check examples/software-architecture/bundle --manifiesto examples/software-architecture/manifest.toml --invariantes examples/software-architecture/invariants/architecture.sx
```

These commands inspect the base only; they do not load or apply the proposal
files. This fixture does not implement `apply`, generate proposals or
corrections, publish a candidate, promise atomicity, or implement recovery.
