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
4. Starting from the corrected state, `proposals/gap/delta.json` removes the
   domain-to-port edge and its `HasLab` atom. `invariants/architecture-gap.sx`
   then reports the missing existential obligation in `states/gap/`. This
   state has three nodes and one remaining edge: it demonstrates a missing
   relation witness, not an empty bundle.

The correction is supplied explicitly by this fixture. Felix evaluates a
proposal; it does not generate the correction. The valid delta is a patch for
the candidate after the contradictory delta, not a patch against the original
base bundle.

The integration test
[`demo_arquitectura`](../../crates/universo-cli/tests/demo_arquitectura.rs)
consumes this durable fixture: it loads the bundle, manifest, both invariants,
and three proposal deltas. Core rejects the contradictory and gap proposals;
the test also builds their candidates in memory and compares them with the
durable snapshots. The concrete mapping from the abstract gap witness to the
removed edge and label is asserted only in the test. Only the corrected
candidate is serialized and reloaded. CLI `validate`/`check` run on that
candidate, and `validate`/`check --explain enumerate` run on the gap snapshot;
the test does not reject a proposal through the CLI or serialize the
reconstructed contradictory or gap candidates.

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

The snapshots under `states/`, including `states/gap/`, are complete,
versioned illustrative states aligned with their deltas; they are not outputs
of `apply` or generated corrections. The correction and the gap-producing
removal are both supplied explicitly by this fixture.

The standard-library runner
[`run_software_architecture_demo.py`](../../tools/run_software_architecture_demo.py)
builds the CLI and checks `validate` and `check` in JSON and text for the four
versioned states against 16 exact outputs in `expected/`. The contradictory
check uses `--explain decide`; the gap check uses `--explain enumerate` so its
abstract, redacted witness and gap summary are part of the goldens. The runner
validates the JSON structure and budget, rejects duplicate object keys, and
checks stdout, stderr, and goldens for concrete fixture markers. These
marker checks cover this public fixture; the CLI witness does not expose its
concrete filenames, atom IDs, labels, or formula.
Run it from the repository root with:

```powershell
python tools/run_software_architecture_demo.py
```

The runner inspects the durable fixture states; it does not implement `apply`,
generate proposals or corrections, publish a candidate, promise atomicity, or
implement recovery.
