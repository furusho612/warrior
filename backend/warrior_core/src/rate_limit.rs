//! 固定の時間窓のレート制限の計算。
//!
//! 窓の開始は現在時刻を窓の秒数で割った商に窓の秒数を掛けた値とする。
//! 上限と窓の秒数は環境変数から読み、このモジュールには置かない。

/// 現在時刻が属する窓の開始 (Unix 秒)。
pub fn window_start(now_epoch_seconds: i64, window_seconds: i64) -> i64 {
    now_epoch_seconds.div_euclid(window_seconds) * window_seconds
}

/// 次の窓までの秒数。窓の開始ちょうどでは窓の秒数を返す。
pub fn retry_after(now_epoch_seconds: i64, window_seconds: i64) -> i64 {
    window_seconds - now_epoch_seconds.rem_euclid(window_seconds)
}

#[cfg(test)]
mod tests {
    use super::{retry_after, window_start};

    #[test]
    fn window_start_is_a_multiple_of_the_window() {
        assert_eq!(window_start(0, 60), 0);
        assert_eq!(window_start(59, 60), 0);
        assert_eq!(window_start(60, 60), 60);
        assert_eq!(window_start(121, 60), 120);
    }

    #[test]
    fn retry_after_is_positive_and_within_the_window() {
        assert_eq!(retry_after(0, 60), 60);
        assert_eq!(retry_after(59, 60), 1);
        assert_eq!(retry_after(60, 60), 60);
    }
}
