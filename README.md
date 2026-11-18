# Offline sandbox (orphan branch)

Built 2026-11-18T15:33:39Z from commit `7d40573` for platform `linux-64`.
`pixi.lock` sha256 `aaf0861333ce9157c19a65dce5476a4d3b2149c4d1e2b51349654f2a118ad82f`.

The verified self-bootstrap binary is stored at `.pixi-sandbox/tools/linux-64/pixi-sandbox`. The branch root intentionally contains documentation only.

| env | platform | packed | unpacked | files |
| --- | --- | ---: | ---: | ---: |
| `default` | linux-64 | 534.9 MiB | 2031.9 MiB | 60 |

Cargo dependencies: **417 crates**, 342.2 MiB (loose) from `Cargo.lock` sha256 `0f676983fc62…`; restore materialises them to `.pixi-sandbox/vendor/`. Built with cargo 1.100.0 (3acc79c0c 2026-11-05); rustc 1.100.0 (0ef1bc386 2026-11-09).


## Restore on the disconnected machine

```bash
./.pixi-sandbox/tools/linux-64/pixi-sandbox doctor --branch-location . --verify
./.pixi-sandbox/tools/linux-64/pixi-sandbox restore --branch-location . --output-path <project> --force
# then, from <project> with no network, use pixi as the sole entrypoint:
pixi install --frozen --offline
pixi run --frozen -- cargo build --offline
```

Every manifest blob is verified before it is written into the working tree.
