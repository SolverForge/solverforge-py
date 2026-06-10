use include_dir::{include_dir, Dir, DirEntry};

static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/static/sf");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiAsset {
    pub path: &'static str,
    pub content_type: &'static str,
    pub cache_control: &'static str,
    pub bytes: &'static [u8],
}

pub fn get(path: &str) -> Option<UiAsset> {
    let path = normalize_path(path)?;
    let file = ASSETS.get_file(path)?;
    let path = file.path().to_str()?;
    Some(UiAsset {
        path,
        content_type: content_type_from_path(path),
        cache_control: cache_control_from_path(path),
        bytes: file.contents(),
    })
}

pub fn paths() -> Vec<&'static str> {
    let mut paths = Vec::new();
    collect_paths(ASSETS.entries(), &mut paths);
    paths.sort_unstable();
    paths
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn collect_paths(entries: &'static [DirEntry<'static>], paths: &mut Vec<&'static str>) {
    for entry in entries {
        match entry {
            DirEntry::Dir(dir) => collect_paths(dir.entries(), paths),
            DirEntry::File(file) => {
                if let Some(path) = file.path().to_str() {
                    paths.push(path);
                }
            }
        }
    }
}

fn normalize_path(path: &str) -> Option<&str> {
    let path = path.trim_start_matches("./");
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    Some(path)
}

pub fn content_type_from_path(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("eot") => "application/vnd.ms-fontobject",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        Some("json") | Some("map") => "application/json",
        Some("html") => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub fn cache_control_from_path(path: &str) -> &'static str {
    if is_immutable(path) {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    }
}

fn is_immutable(path: &str) -> bool {
    path.starts_with("fonts/")
        || path.starts_with("vendor/")
        || path.starts_with("img/")
        || is_versioned_bundle(path)
}

fn is_versioned_bundle(path: &str) -> bool {
    path.strip_prefix("sf.")
        .and_then(|rest| rest.rsplit_once('.'))
        .map(|(version, ext)| {
            !version.is_empty()
                && version.chars().all(|ch| {
                    ch.is_ascii_digit()
                        || ch == '.'
                        || ch == '-'
                        || ch == '+'
                        || ch.is_ascii_alphabetic()
                })
                && matches!(ext, "css" | "js" | "mjs")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_assets_with_metadata() {
        let asset = get("sf.js").expect("sf.js should be embedded");
        assert_eq!(asset.path, "sf.js");
        assert_eq!(asset.content_type, "application/javascript; charset=utf-8");
        assert_eq!(asset.cache_control, "public, max-age=3600");
        assert!(!asset.bytes.is_empty());

        let css = get("sf.css").expect("sf.css should be embedded");
        assert_eq!(css.content_type, "text/css; charset=utf-8");
    }

    #[test]
    fn returns_version_and_paths() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        let paths = paths();
        assert!(paths.contains(&"sf.js"));
        assert!(paths.contains(&"sf.css"));
        assert!(paths.contains(&"img/ouroboros.svg"));
    }

    #[test]
    fn rejects_unsafe_paths() {
        assert!(get("").is_none());
        assert!(get("/sf.js").is_none());
        assert!(get("../sf.js").is_none());
        assert!(get("vendor/../sf.js").is_none());
        assert!(get(r"vendor\leaflet\leaflet.js").is_none());
    }

    #[test]
    fn cache_and_content_type_rules_match_public_routes() {
        assert_eq!(
            content_type_from_path("styles/sf.css"),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            content_type_from_path("scripts/sf.js"),
            "application/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type_from_path("scripts/sf.mjs"),
            "application/javascript; charset=utf-8"
        );
        assert_eq!(content_type_from_path("img/logo.svg"), "image/svg+xml");
        assert_eq!(content_type_from_path("data.json"), "application/json");
        assert_eq!(content_type_from_path("bundle.map"), "application/json");

        assert_eq!(
            cache_control_from_path("fonts/jetbrains-mono.woff2"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            cache_control_from_path("vendor/leaflet/leaflet.js"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            cache_control_from_path("img/ouroboros.svg"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            cache_control_from_path("sf.0.6.5.css"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(cache_control_from_path("sf.css"), "public, max-age=3600");
    }
}
