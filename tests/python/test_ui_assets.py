from solverforge.ui import asset, asset_paths


def test_solverforge_ui_assets_are_served_from_native_bridge() -> None:
    sf_js = asset("sf.js")
    assert sf_js is not None
    assert sf_js.path == "sf.js"
    assert sf_js.content_type == "application/javascript; charset=utf-8"
    assert sf_js.cache_control == "public, max-age=3600"
    assert b"sf.createBackend = function" in sf_js.bytes

    versioned_css = asset("sf.0.6.5.css")
    assert versioned_css is not None
    assert versioned_css.content_type == "text/css; charset=utf-8"
    assert versioned_css.cache_control == "public, max-age=31536000, immutable"

    logo = asset("img/ouroboros.svg")
    assert logo is not None
    assert logo.content_type == "image/svg+xml"
    assert logo.cache_control == "public, max-age=31536000, immutable"


def test_solverforge_ui_assets_preserve_legacy_module_bundle_names() -> None:
    sf_mjs = asset("sf.mjs")
    assert sf_mjs is not None
    assert sf_mjs.path == "sf.mjs"
    assert sf_mjs.content_type == "application/javascript; charset=utf-8"
    assert sf_mjs.cache_control == "public, max-age=3600"
    assert b"export {" in sf_mjs.bytes
    assert b"createBackend" in sf_mjs.bytes

    versioned_mjs = asset("sf.0.6.5.mjs")
    assert versioned_mjs is not None
    assert versioned_mjs.path == "sf.0.6.5.mjs"
    assert versioned_mjs.content_type == "application/javascript; charset=utf-8"
    assert versioned_mjs.cache_control == "public, max-age=31536000, immutable"
    assert versioned_mjs.bytes == sf_mjs.bytes


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
    assert "sf.0.6.5.mjs" in paths
    assert "sf.css" in paths
    assert "modules/sf-map.js" in paths
    assert "vendor/leaflet/leaflet.js" in paths
