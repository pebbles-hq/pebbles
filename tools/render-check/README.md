# render-check

Pixel render-check harness for the wgpu migration. Renders every corpus case with the
candidate renderer (`pebbles-gpu`) and judges it against the raster spec: the golden model,
analytic probes and committed self-goldens. Dev tool; never shipped.

Run from the repository root:

```bash
cargo run -p render-check --release -- --list
cargo run -p render-check --release -- --out target/render-check                           # RENDER-CHECK
cargo run -p render-check --release -- --out target/render-check --only stroke/ --only prim/
cargo run -p render-check --release -- --out target/render-check --reference dir           # vs committed refs
```

Output: `target/render-check/<platform-tag>/{report.md,report.json,cases/}`.

Exit codes: `0` every case PASS / XFAIL / UNJUDGED / filtered out; `1` any FAIL, XPASS, PANIC,
GPU_ERROR, UNSTABLE, HANG, SIZE, MISSING_REF, PROBE_FAIL; `2` usage or I/O error; `3` no GPU
adapter.

Design, case model, metric and corpus: see `documentations/wgpu-migration/05-testing-and-ci.md`
§6 (harness) and §7 (corpus).
