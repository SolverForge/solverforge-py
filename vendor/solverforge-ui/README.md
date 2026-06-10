# solverforge-ui vendored crate

This directory is the vendored `solverforge-ui` `0.6.5` crate used by
`solverforge-py`. It embeds the shared `/sf/*` frontend assets into the Python
native extension so source-checkout examples can serve one shared UI bundle
without copying it into each example tree.

The vendored copy intentionally contains only the crate surface needed by this
repository:

- `Cargo.toml` and `Cargo.lock`
- `src/lib.rs` with the Axum route helper
- `src/assets.rs` with the host-neutral embedded asset API
- `static/sf/` with stable and versioned CSS/JavaScript bundles, fonts, logo,
  optional map module assets, and third-party browser vendors

The upstream `solverforge-ui` development repository owns source CSS/JS,
screenshots, bundling scripts, and frontend-specific lint/test targets. Those
development-only files are not part of this vendored runtime crate.

## Rust API

Axum applications can mount the shared asset route directly:

```rust
let app = api::router(state)
    .merge(solverforge_ui::routes());
```

That serves assets under `/sf/{path}` with content-type and cache-control
headers from the embedded metadata.

Non-Axum hosts should use the asset module:

```rust
let asset = solverforge_ui::assets::get("sf.js").expect("asset exists");
assert_eq!(asset.path, "sf.js");
assert_eq!(asset.content_type, "application/javascript; charset=utf-8");
```

`solverforge_ui::assets::paths()` returns the embedded path list, and
`solverforge_ui::assets::version()` returns the crate version.

## Python Bridge

`solverforge-py` exposes this crate through `solverforge.ui`:

```python
from solverforge.ui import asset, asset_paths

sf_js = asset("sf.js")
assert sf_js is not None
assert sf_js.content_type == "application/javascript; charset=utf-8"
assert "vendor/leaflet/leaflet.js" in asset_paths()
```

The hospital and deliveries FastAPI examples serve `/sf/{path}` from this
bridge and keep only app-specific browser code under their own `static/`
directories.

## Asset Paths

Stable compatibility paths include:

- `sf.css`
- `sf.js`
- `sf.mjs`

Versioned immutable paths include:

- `sf.0.6.5.css`
- `sf.0.6.5.js`
- `sf.0.6.5.mjs`

Additional embedded paths include:

- `fonts/jetbrains-mono.woff2`
- `fonts/space-grotesk.woff2`
- `img/ouroboros.svg`
- `modules/sf-map.css`
- `modules/sf-map.js`
- `vendor/fontawesome/...`
- `vendor/frappe-gantt/...`
- `vendor/leaflet/...`
- `vendor/split/split.min.js`

Unsafe paths are rejected: empty paths, absolute paths, backslash paths, and
segments containing `.` or `..` all return `None`.

## Cache And Content Types

Immutable cache headers are used for fonts, vendor assets, images, and versioned
bundles:

```text
public, max-age=31536000, immutable
```

Stable top-level compatibility bundles use a shorter cache:

```text
public, max-age=3600
```

Content types are inferred from embedded path extensions. CSS and JavaScript are
served with UTF-8 text content types, SVG as `image/svg+xml`, web fonts with
font content types, and JSON/map files as `application/json`.

## Validation

From the `solverforge-py` root:

```sh
make test-quick
make docs-check
```

The Python UI bridge is covered by `tests/python/test_ui_assets.py`. To test the
vendored crate directly:

```sh
cd vendor/solverforge-ui
cargo test --locked
```
