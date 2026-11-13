//! 性質テストで共有する入力の生成。

use proptest::prelude::*;

/// 小文字の 16 進 1 文字。
pub fn hex_digit() -> impl Strategy<Value = char> {
    prop::sample::select("0123456789abcdef".chars().collect::<Vec<char>>())
}

/// 指定した長さの小文字の 16 進の文字列。
fn hex_segment(length: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(hex_digit(), length)
        .prop_map(|characters| characters.into_iter().collect())
}

/// バージョン 4 と RFC 4122 の variant を持つ UUID の文字列。
pub fn uuid_v4_text() -> impl Strategy<Value = String> {
    (
        hex_segment(8),
        hex_segment(4),
        hex_segment(4),
        hex_segment(4),
        hex_segment(12),
        prop::sample::select(vec!['8', '9', 'a', 'b']),
    )
        .prop_map(|(first, second, third, fourth, fifth, variant)| {
            let third = format!("4{}", &third[1..]);
            let fourth = format!("{variant}{}", &fourth[1..]);
            format!("{first}-{second}-{third}-{fourth}-{fifth}")
        })
}
