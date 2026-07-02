use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

const JAVASCRIPT_CONTENT_TYPE: &str = "application/javascript; charset=utf-8";
const SHORT_CACHE_CONTROL: &str = "public, max-age=3600";
const MODULE_ASSET_PATHS: &[&str] = &["sf.mjs"];
const MODULE_BUNDLE: &[u8] = br#"const sfCacheKey = Symbol.for('solverforge.ui.sf.module');
let sfPromise = globalThis[sfCacheKey];

if (!sfPromise) {
  sfPromise = (async () => {
    const owns = Object.prototype.hasOwnProperty;
    const hadWindow = owns.call(globalThis, 'window');
    const previousWindow = globalThis.window;
    const sfTarget = hadWindow && previousWindow ? previousWindow : globalThis;
    const hadGlobalSF = owns.call(globalThis, 'SF');
    const previousGlobalSF = globalThis.SF;
    const hadWindowSF = sfTarget !== globalThis && owns.call(sfTarget, 'SF');
    const previousWindowSF = sfTarget.SF;

    if (!hadWindow) {
      Object.defineProperty(globalThis, 'window', {
        configurable: true,
        value: globalThis,
        writable: true,
      });
    }

    let loadedSf;
    try {
      const moduleUrl = new URL(import.meta.url);
      await import('./sf.js?solverforge-ui-module=' + encodeURIComponent(moduleUrl.pathname));
      loadedSf = globalThis.SF || (globalThis.window && globalThis.window.SF);
      if (!loadedSf) {
        throw new Error('[SolverForge] /sf/sf.js did not initialize the global SF object');
      }
    } finally {
      if (sfTarget !== globalThis) {
        if (!hadWindowSF) {
          delete sfTarget.SF;
        } else {
          sfTarget.SF = previousWindowSF;
        }
      }
      if (!hadGlobalSF) {
        delete globalThis.SF;
      } else {
        globalThis.SF = previousGlobalSF;
      }
      if (!hadWindow) {
        delete globalThis.window;
      }
    }

    return loadedSf;
  })();
  Object.defineProperty(globalThis, sfCacheKey, {
    configurable: true,
    value: sfPromise,
    writable: false,
  });
}

let sf;
try {
  sf = await sfPromise;
} catch (error) {
  if (globalThis[sfCacheKey] === sfPromise) {
    delete globalThis[sfCacheKey];
  }
  throw error;
}

const assert = sf.assert;
const bindActivation = sf.bindActivation;
const colorClass = sf.score.colorClass;
const colors = sf.colors;
const createApiGuide = sf.createApiGuide;
const createBackend = sf.createBackend;
const createButton = sf.createButton;
const createFooter = sf.createFooter;
const createHeader = sf.createHeader;
const createModal = sf.createModal;
const createSolver = sf.createSolver;
const createStatusBar = sf.createStatusBar;
const createTable = sf.createTable;
const createTabs = sf.createTabs;
const el = sf.el;
const escHtml = sf.escHtml;
const gantt = sf.gantt;
const getComponents = sf.score.getComponents;
const normalizeCreateJobId = sf.normalizeCreateJobId;
const parseHard = sf.score.parseHard;
const parseMedium = sf.score.parseMedium;
const parseSoft = sf.score.parseSoft;
const pick = sf.colors.pick;
const project = sf.colors.project;
const rail = sf.rail;
const reset = sf.colors.reset;
const score = sf.score;
const showError = sf.showError;
const showTab = sf.showTab;
const showToast = sf.showToast;
const uid = sf.uid;
const version = sf.version;

export {
  assert,
  bindActivation,
  colorClass,
  colors,
  createApiGuide,
  createBackend,
  createButton,
  createFooter,
  createHeader,
  createModal,
  createSolver,
  createStatusBar,
  createTable,
  createTabs,
  el,
  escHtml,
  gantt,
  getComponents,
  normalizeCreateJobId,
  parseHard,
  parseMedium,
  parseSoft,
  pick,
  project,
  rail,
  reset,
  score,
  showError,
  showTab,
  showToast,
  uid,
  version,
};

export default sf;
"#;

#[pyfunction]
pub fn ui_asset(py: Python<'_>, path: &str) -> PyResult<Option<Py<PyAny>>> {
    if path == "sf.mjs" {
        return asset_dict(
            py,
            path,
            JAVASCRIPT_CONTENT_TYPE,
            SHORT_CACHE_CONTROL,
            MODULE_BUNDLE,
        )
        .map(Some);
    }

    let Ok(asset) = solverforge_ui::assets::get(path) else {
        return Ok(None);
    };

    asset_dict(
        py,
        asset.path(),
        asset.content_type(),
        asset.cache_control(),
        asset.bytes(),
    )
    .map(Some)
}

#[pyfunction]
pub fn ui_asset_paths() -> Vec<&'static str> {
    let mut paths = solverforge_ui::assets::paths().to_vec();
    paths.extend_from_slice(MODULE_ASSET_PATHS);
    paths.sort_unstable();
    paths.dedup();
    paths
}

fn asset_dict(
    py: Python<'_>,
    path: &str,
    content_type: &str,
    cache_control: &str,
    bytes: &[u8],
) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("path", path)?;
    dict.set_item("content_type", content_type)?;
    dict.set_item("cache_control", cache_control)?;
    dict.set_item("bytes", PyBytes::new(py, bytes))?;
    Ok(dict.into_any().unbind())
}
