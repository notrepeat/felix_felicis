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

The integration test
[`demo_arquitectura`](../../crates/universo-cli/tests/demo_arquitectura.rs)
consumes this durable fixture: it loads the bundle, manifest, invariant, and
both proposal deltas. It rejects the contradictory proposal in core, builds
that contradictory candidate in memory to evaluate the explicit correction,
then serializes and reloads only the corrected candidate. The CLI
`validate`/`check` calls in the test run only on that corrected candidate; the
test does not reject this proposal through the CLI or serialize the
reconstructed in-memory contradictory candidate.

`states/contradictory/` and `states/corrected/` are complete, versioned,
illustrative snapshots for the states after the respective deltas. The test
loads and compares them with candidates computed from the base and deltas.
They are checked-in fixture data, not outputs of `apply` or generated
corrections.

These CLI commands inspect only the base bundle. They do not load or apply the
proposal files:

```powershell
cargo run -p universo-cli -- validate examples/software-architecture/bundle --manifiesto examples/software-architecture/manifest.toml
cargo run -p universo-cli -- check examples/software-architecture/bundle --manifiesto examples/software-architecture/manifest.toml --invariantes examples/software-architecture/invariants/architecture.sx
```

The standard-library runner
[`run_software_architecture_demo.py`](../../tools/run_software_architecture_demo.py)
builds the CLI and checks `validate` and `check` in JSON and text for all three
versioned states against the exact outputs in `expected/`. The contradictory
check uses `--explain decide` so its redacted witness is part of the golden.
Run it from the repository root with:

```powershell
python tools/run_software_architecture_demo.py
```

The runner inspects the durable fixture states; it does not implement `apply`,
generate proposals or corrections, publish a candidate, promise atomicity, or
implement recovery.
