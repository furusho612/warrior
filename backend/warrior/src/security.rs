//! Worker の応答に付けるセキュリティヘッダと、同一オリジンの検証。
//!
//! ヘッダの組み立ては純粋な定数とし、応答への付与は [`apply_security_headers`] が行う。
//! 静的ファイルの応答は Worker を通らないため、同じ値を `frontend/public/_headers` に置く。

use worker::{Headers, Method, Request, Response, Result, Url};

/// 外部スクリプトを許可しない CSP。WASM の実行と Google Fonts だけを許可する。
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

/// HSTS。
pub const STRICT_TRANSPORT_SECURITY: &str = "max-age=31536000; includeSubDomains";

/// コンテンツの種類を推測させない。
pub const X_CONTENT_TYPE_OPTIONS: &str = "nosniff";

/// 参照元の情報を抑える。
pub const REFERRER_POLICY: &str = "strict-origin-when-cross-origin";

/// 使わない端末の機能を無効にする。
pub const PERMISSIONS_POLICY: &str = "camera=(), microphone=(), geolocation=(), payment=(), usb=()";

/// Worker の応答と静的ファイルの応答に付けるヘッダ。
pub const SECURITY_HEADERS: &[(&str, &str)] = &[
    ("Content-Security-Policy", CONTENT_SECURITY_POLICY),
    ("Strict-Transport-Security", STRICT_TRANSPORT_SECURITY),
    ("X-Content-Type-Options", X_CONTENT_TYPE_OPTIONS),
    ("Referrer-Policy", REFERRER_POLICY),
    ("Permissions-Policy", PERMISSIONS_POLICY),
];

/// 応答にセキュリティヘッダを付ける。CORS のヘッダは付けない。
pub fn apply_security_headers(response: &mut Response) -> Result<()> {
    let headers: &mut Headers = response.headers_mut();
    for (name, value) in SECURITY_HEADERS {
        headers.set(name, value)?;
    }
    Ok(())
}

/// 状態を変更するメソッドか。`GET` と `HEAD` は `Origin` の検証をしない。
pub fn changes_state(method: &Method) -> bool {
    matches!(
        method,
        Method::Post | Method::Put | Method::Patch | Method::Delete
    )
}

/// `Origin` が環境ごとの Origin と一致するか。
pub fn is_same_origin(request: &Request, expected_origin: &str) -> Result<bool> {
    let origin = request.headers().get("Origin")?;
    is_same_origin_value(origin.as_deref(), expected_origin)
}

/// `Origin` の文字列と期待する Origin が同じオリジンかを判定する。
///
/// scheme、host、port を URL として正規化して比較する。
/// `Origin` が無い、または URL として読めない場合は一致しない扱いにする。
pub fn is_same_origin_value(origin: Option<&str>, expected_origin: &str) -> Result<bool> {
    let Some(origin) = origin else {
        return Ok(false);
    };
    let Ok(origin) = Url::parse(origin) else {
        return Ok(false);
    };
    Ok(origin.origin() == Url::parse(expected_origin)?.origin())
}

#[cfg(test)]
mod tests {
    use super::{changes_state, is_same_origin_value, CONTENT_SECURITY_POLICY, SECURITY_HEADERS};
    use worker::Method;

    const STATIC_HEADERS: &str = include_str!("../../../frontend/public/_headers");

    #[test]
    fn every_security_header_is_present_in_the_static_headers_file() {
        for (name, value) in SECURITY_HEADERS {
            let line = format!("{name}: {value}");
            assert!(
                STATIC_HEADERS
                    .lines()
                    .any(|candidate| candidate.trim() == line),
                "static headers are missing {line}"
            );
        }
    }

    #[test]
    fn static_headers_do_not_allow_cors() {
        assert!(!STATIC_HEADERS
            .to_ascii_lowercase()
            .contains("access-control-allow-origin"));
    }

    #[test]
    fn worker_headers_do_not_allow_cors() {
        for (name, _) in SECURITY_HEADERS {
            assert_ne!(name.to_ascii_lowercase(), "access-control-allow-origin");
        }
    }

    #[test]
    fn csp_allows_wasm_and_google_fonts_only() {
        assert!(CONTENT_SECURITY_POLICY.contains("script-src 'self' 'wasm-unsafe-eval'"));
        assert!(CONTENT_SECURITY_POLICY.contains("https://fonts.googleapis.com"));
        assert!(CONTENT_SECURITY_POLICY.contains("font-src https://fonts.gstatic.com"));
        assert!(!CONTENT_SECURITY_POLICY.contains("script-src 'self' 'wasm-unsafe-eval' https://"));
        assert!(!CONTENT_SECURITY_POLICY.contains("'unsafe-eval'"));
    }

    #[test]
    fn post_put_patch_delete_change_state() {
        for method in [Method::Post, Method::Put, Method::Patch, Method::Delete] {
            assert!(changes_state(&method), "{method:?} must change state");
        }
    }

    #[test]
    fn get_and_head_do_not_change_state() {
        assert!(!changes_state(&Method::Get));
        assert!(!changes_state(&Method::Head));
    }

    #[test]
    fn same_origin_passes() {
        assert!(
            is_same_origin_value(Some("http://localhost:8787"), "http://localhost:8787").unwrap()
        );
        assert!(is_same_origin_value(
            Some("https://warrior.example.workers.dev"),
            "https://warrior.example.workers.dev"
        )
        .unwrap());
    }

    #[test]
    fn different_scheme_host_or_port_fails() {
        assert!(
            !is_same_origin_value(Some("https://localhost:8787"), "http://localhost:8787").unwrap()
        );
        assert!(!is_same_origin_value(
            Some("https://evil.example"),
            "https://warrior.example.workers.dev"
        )
        .unwrap());
        assert!(
            !is_same_origin_value(Some("http://localhost:1"), "http://localhost:8787").unwrap()
        );
    }

    #[test]
    fn default_ports_are_normalized() {
        assert!(
            is_same_origin_value(Some("https://example.com:443"), "https://example.com").unwrap()
        );
        assert!(
            !is_same_origin_value(Some("https://example.com"), "https://example.com:8443").unwrap()
        );
    }

    #[test]
    fn missing_or_invalid_origin_fails() {
        assert!(!is_same_origin_value(None, "https://example.com").unwrap());
        assert!(!is_same_origin_value(Some("not a url"), "https://example.com").unwrap());
        assert!(!is_same_origin_value(Some("null"), "https://example.com").unwrap());
    }

    #[test]
    fn invalid_expected_origin_is_an_error() {
        assert!(is_same_origin_value(Some("https://example.com"), "not a url").is_err());
    }
}
