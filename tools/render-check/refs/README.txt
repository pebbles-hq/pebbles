Committed self-goldens (05 §8.3–8.4, DR-21): render-check's own output for every `in_refs` case,
judged against with `--reference dir`.

wgpu/linux-lavapipe/
  generated: locally (GitHub Actions cannot start jobs on this account), 2026-10-24
  commit:    4be9fb8 + the --write-refs change that also writes passing-probe self-golden cases
  adapter:   llvmpipe (LLVM 20.1.2, 256 bits) | Vulkan (lavapipe) | Cpu
  mesa:      mesa-vulkan-drivers 25.2.8-0ubuntu0.24.04.2
  cases:     302
  bytes:     2604199
  command:   cargo run -p render-check --release -- --out target/render-check --write-refs <dir> --determinism

Every case passed the golden model, or (widget/screen/stress self-golden cases) had green probes
and was deterministic. Review each image in target/render-check/<platform>/report.md before
relying on it. Regenerate only via a dedicated PR (05 §9.4) — never together with renderer
changes, never on a hardware GPU.
