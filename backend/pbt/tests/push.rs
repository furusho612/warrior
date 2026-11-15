//! Web Push の endpoint の検証の性質テスト。
//!
//! 対応する要求: `REQ-TMR-008`。
//! 受理する endpoint は HTTPS で、ホストが許可リストのいずれかであること。
//! 許可したホストの HTTPS の endpoint は、経路と問い合わせが何であっても受理すること。
//! それ以外の endpoint は拒否することを確かめる。

use proptest::prelude::*;
use warrior_core::push::{is_allowed_endpoint, ALLOWED_HOSTS};

fn any_scheme() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "https", "HTTPS", "Https", "http", "HTTP", "ftp", "ws", "",
    ])
}

fn any_host() -> impl Strategy<Value = String> {
    prop_oneof![
        prop::sample::select(ALLOWED_HOSTS.to_vec()).prop_map(str::to_owned),
        Just("WEB.PUSH.APPLE.COM".to_owned()),
        Just("evil.example".to_owned()),
        Just("web.push.apple.com.evil.example".to_owned()),
        Just("sub.fcm.googleapis.com".to_owned()),
        Just("fcm.googleapis.com.".to_owned()),
        Just(String::new()),
    ]
}

fn any_endpoint() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..120).prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn req_tmr_008_only_https_on_an_allowed_host_is_accepted(
        scheme in any_scheme(),
        host in any_host(),
        path in "/[A-Za-z0-9._~!$&'()*+,;=:@%/-]{0,80}",
    ) {
        let endpoint = format!("{scheme}://{host}{path}");
        let accepted = is_allowed_endpoint(&endpoint);
        let expected = scheme.eq_ignore_ascii_case("https")
            && ALLOWED_HOSTS.iter().any(|allowed| host.eq_ignore_ascii_case(allowed));
        prop_assert_eq!(accepted, expected, "endpoint {}", endpoint);
    }

    #[test]
    fn req_tmr_008_every_path_and_query_of_an_allowed_host_is_accepted(
        host_index in 0..ALLOWED_HOSTS.len(),
        path in "/[A-Za-z0-9._~!$&'()*+,;=:@%/-]{0,100}",
        query in "[A-Za-z0-9=&%_-]{0,40}",
        fragment in "[A-Za-z0-9=&%_-]{0,40}",
    ) {
        let host = ALLOWED_HOSTS[host_index];
        let mut endpoint = format!("https://{host}{path}");
        if !query.is_empty() {
            endpoint.push('?');
            endpoint.push_str(&query);
        }
        if !fragment.is_empty() {
            endpoint.push('#');
            endpoint.push_str(&fragment);
        }
        prop_assert!(is_allowed_endpoint(&endpoint), "endpoint {}", endpoint);
    }

    #[test]
    fn req_tmr_008_an_accepted_endpoint_is_https_on_an_allowed_host(endpoint in any_endpoint()) {
        if is_allowed_endpoint(&endpoint) {
            let (scheme, rest) = endpoint.split_once("://").expect("an accepted endpoint has a scheme");
            prop_assert!(scheme.eq_ignore_ascii_case("https"), "endpoint {}", endpoint);
            let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
            let host = authority.split(':').next().unwrap_or("");
            prop_assert!(
                ALLOWED_HOSTS.iter().any(|allowed| host.eq_ignore_ascii_case(allowed)),
                "endpoint {} has the host {}",
                endpoint,
                host
            );
        }
    }
}
