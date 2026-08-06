import shutil
import subprocess

import pytest
from solverforge.ui import asset, asset_paths


def test_solverforge_ui_assets_are_served_from_native_bridge() -> None:
    sf_js = asset("sf.js")
    assert sf_js is not None
    assert sf_js.path == "sf.js"
    assert sf_js.content_type == "application/javascript; charset=utf-8"
    assert sf_js.cache_control == "public, max-age=3600"
    assert b"sf.createBackend = function" in sf_js.bytes

    versioned_css = asset("sf.0.7.0.css")
    assert versioned_css is not None
    assert versioned_css.content_type == "text/css; charset=utf-8"
    assert versioned_css.cache_control == "public, max-age=31536000, immutable"

    logo = asset("img/ouroboros.svg")
    assert logo is not None
    assert logo.content_type == "image/svg+xml"
    assert logo.cache_control == "public, max-age=31536000, immutable"


def test_solverforge_ui_assets_serve_current_versioned_bundle() -> None:
    versioned_js = asset("sf.0.7.0.js")
    assert versioned_js is not None
    assert versioned_js.path == "sf.0.7.0.js"
    assert versioned_js.content_type == "application/javascript; charset=utf-8"
    assert versioned_js.cache_control == "public, max-age=31536000, immutable"
    assert b"sf.createBackend = function" in versioned_js.bytes


def test_solverforge_ui_assets_do_not_alias_stale_or_synthetic_versioned_bundles() -> (
    None
):
    assert asset("sf.0.6.6.css") is None
    assert asset("sf.0.6.6.js") is None
    assert asset("sf.0.6.6.mjs") is None
    assert asset("sf.0.7.0.mjs") is None


def test_solverforge_ui_assets_expose_only_unversioned_module_wrapper() -> None:
    sf_mjs = asset("sf.mjs")
    assert sf_mjs is not None
    assert sf_mjs.path == "sf.mjs"
    assert sf_mjs.content_type == "application/javascript; charset=utf-8"
    assert sf_mjs.cache_control == "public, max-age=3600"
    assert b"await import('./sf.js?solverforge-ui-module='" in sf_mjs.bytes
    assert b"Object.defineProperty(globalThis, 'window'" in sf_mjs.bytes
    assert b"const parseHard = sf.score.parseHard;" in sf_mjs.bytes
    assert b"const pick = sf.colors.pick;" in sf_mjs.bytes
    assert b"createBackend" in sf_mjs.bytes


def test_solverforge_ui_module_imports_without_window(tmp_path) -> None:
    node = shutil.which("node")
    if node is None:
        pytest.skip("node executable is required for the non-window ESM import check")

    sf_js = asset("sf.js")
    sf_mjs = asset("sf.mjs")
    assert sf_js is not None
    assert sf_mjs is not None

    (tmp_path / "package.json").write_text('{"type": "module"}\n', encoding="utf-8")
    (tmp_path / "sf.js").write_bytes(sf_js.bytes)
    (tmp_path / "sf.mjs").write_bytes(sf_mjs.bytes)
    check = tmp_path / "check.mjs"
    check.write_text(
        """
import sf, { createBackend, parseHard, pick, version } from './sf.mjs';

if (typeof globalThis.window !== 'undefined') {
  throw new Error('module wrapper leaked a synthetic window global');
}
if (typeof globalThis.SF !== 'undefined') {
  throw new Error('module wrapper leaked a synthetic SF global');
}
if (!sf || createBackend !== sf.createBackend) {
  throw new Error('module wrapper default export does not match sf.js globals');
}
if (parseHard !== sf.score.parseHard || pick !== sf.colors.pick) {
  throw new Error('module wrapper did not preserve score/color exports');
}

console.log(`${version}:${typeof createBackend}`);
""".strip() + "\n",
        encoding="utf-8",
    )

    completed = subprocess.run(
        [node, str(check)],
        cwd=tmp_path,
        check=True,
        capture_output=True,
        text=True,
    )
    assert completed.stdout.strip() == "0.7.0:function"


def test_solverforge_ui_asset_bridge_rejects_unsafe_paths() -> None:
    assert asset("") is None
    assert asset("/sf.js") is None
    assert asset("../sf.js") is None
    assert asset("vendor/../sf.js") is None
    assert asset(r"vendor\leaflet\leaflet.js") is None


def test_solverforge_ui_asset_paths_are_available() -> None:
    paths = asset_paths()
    assert "sf.js" in paths
    assert "sf.mjs" in paths
    assert "sf.0.7.0.css" in paths
    assert "sf.0.7.0.js" in paths
    assert "sf.css" in paths
    assert "modules/sf-map.js" in paths
    assert "vendor/leaflet/leaflet.js" in paths

    assert "sf.0.6.6.css" not in paths
    assert "sf.0.6.6.js" not in paths
    assert "sf.0.6.6.mjs" not in paths
    assert "sf.0.7.0.mjs" not in paths
