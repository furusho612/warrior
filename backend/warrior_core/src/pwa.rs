//! PWA のキャッシュの版と対象の判定。
//!
//! キャッシュのキーは版と経路から一意に決める。生成された JavaScript は
//! HTTP キャッシュに任せ、Service Worker の Cache API には入れない (ADR-020)。

/// キャッシュ名。
pub fn cache_name(version: &str) -> String {
    format!("warrior-static-{version}")
}

/// キャッシュのキー。
pub fn cache_key(version: &str, path: &str) -> String {
    format!("{version}:{}", normalize_path(path))
}

/// 経路をキャッシュのキー用に正規化する。クエリとフラグメントは落とす。
pub fn normalize_path(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let path = path
        .split_once("://")
        .map(|(_, rest)| rest.find('/').map_or("/", |index| &rest[index..]))
        .unwrap_or(path);
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

/// Service Worker が Cache API に入れる経路か。
///
/// 生成された JavaScript (`/assets/*.js`) と Service Worker のローダーは対象外とする。
/// ルートの画面、WASM、CSS、アイコン、manifest は対象とする。
pub fn is_cacheable_path(path: &str) -> bool {
    let path = normalize_path(path);
    if path.starts_with("/assets/") && path.ends_with(".js") {
        return false;
    }
    if path == "/sw.js" || path.starts_with("/sw/") && path.ends_with(".js") {
        return false;
    }
    path == "/"
        || path.ends_with(".wasm")
        || path.ends_with(".css")
        || path.ends_with(".webmanifest")
        || path.ends_with(".svg")
        || path.ends_with(".png")
        || path == "/passkey.js"
}

#[cfg(test)]
mod tests {
    use super::{cache_key, cache_name, is_cacheable_path};

    #[test]
    fn cache_name_and_key_are_versioned() {
        assert_eq!(cache_name("abc"), "warrior-static-abc");
        assert_eq!(cache_key("abc", "/app.wasm?x=1"), "abc:/app.wasm");
    }

    #[test]
    fn generated_javascript_is_not_cached() {
        assert!(!is_cacheable_path("/assets/app-123.js"));
        assert!(is_cacheable_path("/assets/app-123.wasm"));
        assert!(is_cacheable_path("/tailwind.css"));
        assert!(is_cacheable_path("/manifest.webmanifest"));
        assert!(is_cacheable_path("/sw/warrior_sw_bg.wasm"));
    }
}
