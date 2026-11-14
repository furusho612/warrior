//! RPE (主観的なきつさ) の範囲、刻み、表記 (REQ-SET-007)。
//!
//! RPE は 6.0 から 10.0 まで 0.5 刻みで記録し、空でもよい。
//! 記録画面とサーバーはこの module の関数だけを使い、範囲と刻みを再実装しない。

/// RPE の入力の誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpeError {
    /// 6.0 から 10.0 の 0.5 刻みでない。
    Invalid,
}

/// RPE が 6.0 から 10.0 まで 0.5 刻みか返す。
pub fn is_valid(rpe: f64) -> bool {
    rpe.is_finite() && (6.0..=10.0).contains(&rpe) && (rpe * 2.0).fract() == 0.0
}

/// 入力の文字列を RPE に変換する。空の文字列は None とする。
pub fn parse(text: &str) -> Result<Option<f64>, RpeError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let value = text.parse::<f64>().map_err(|_| RpeError::Invalid)?;
    if !is_valid(value) {
        return Err(RpeError::Invalid);
    }
    Ok(Some(value))
}

/// RPE を 6.0 から 10.0 の 1 桁の小数の表記にする。
pub fn format(rpe: f64) -> String {
    format!("{rpe:.1}")
}

#[cfg(test)]
mod tests {
    //! 対応する要求: `REQ-SET-007`。

    use super::{format, is_valid, parse, RpeError};

    #[test]
    fn req_set_007_accepts_six_to_ten_in_half_steps() {
        for rpe in [6.0, 6.5, 7.0, 8.5, 10.0] {
            assert!(is_valid(rpe), "{rpe} must be valid");
            assert_eq!(parse(&format(rpe)), Ok(Some(rpe)));
        }
    }

    #[test]
    fn req_set_007_accepts_an_empty_rpe() {
        assert_eq!(parse(""), Ok(None));
        assert_eq!(parse("  "), Ok(None));
    }

    #[test]
    fn req_set_007_rejects_values_outside_the_range_and_other_steps() {
        for rpe in [5.5, 10.5, 7.25, 6.25, 6.1, 9.9] {
            assert!(!is_valid(rpe), "{rpe} must be invalid");
            assert_eq!(parse(&format(rpe)), Err(RpeError::Invalid));
        }
        assert_eq!(parse("invalid"), Err(RpeError::Invalid));
        assert!(!is_valid(f64::NAN));
        assert!(!is_valid(f64::INFINITY));
    }

    #[test]
    fn req_set_007_formats_rpe_with_one_decimal() {
        assert_eq!(format(6.0), "6.0");
        assert_eq!(format(6.5), "6.5");
        assert_eq!(format(10.0), "10.0");
    }
}
