//! メモの長さの検証と、画面に表示するときの文字の扱い (REQ-REC-007、REQ-NFR-019)。
//!
//! メモは 2000 文字以下とする。文字数は Unicode のスカラー値で数え、バイト数では数えない。
//! 画面ではテキストとして表示し、HTML として解釈させない。

/// メモの最大の文字数 (REQ-NFR-019)。
pub const MEMO_MAX_CHARS: usize = 2000;

/// メモの長さの誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoError {
    /// 上限の文字数を超える。
    TooLong,
}

/// メモが上限の文字数以内か確かめる。
pub fn validate(memo: &str) -> Result<(), MemoError> {
    if memo.chars().count() > MEMO_MAX_CHARS {
        return Err(MemoError::TooLong);
    }
    Ok(())
}

/// メモを画面のテキストとして返す。
///
/// Dioxus のテキスト補間は HTML を解釈しない。タグの文字はそのまま表示し、
/// 改行とタブを残し、表示を崩す制御文字だけを取り除く。
pub fn display_text(memo: &str) -> String {
    memo.chars()
        .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    //! メモの長さと表示の例示テスト。
    //!
    //! 対応する要求: `REQ-REC-007`、`REQ-NFR-019`。

    use super::{display_text, validate, MemoError, MEMO_MAX_CHARS};

    #[test]
    fn an_empty_memo_is_valid() {
        assert_eq!(validate(""), Ok(()));
        assert_eq!(display_text(""), "");
    }

    #[test]
    fn a_memo_with_newlines_keeps_them_for_display() {
        let memo = "1 セット目\n2 セット目\r\n3 セット目";
        assert_eq!(validate(memo), Ok(()));
        assert_eq!(display_text(memo), "1 セット目\n2 セット目\n3 セット目");
    }

    #[test]
    fn a_japanese_memo_without_emoji_is_valid() {
        let memo = "腰に痛みがあるので、次回は重量を下げる。";
        assert_eq!(validate(memo), Ok(()));
        assert_eq!(display_text(memo), memo);
    }

    #[test]
    fn the_limit_accepts_2000_characters_and_rejects_2001() {
        let at_the_limit = "あ".repeat(MEMO_MAX_CHARS);
        assert_eq!(validate(&at_the_limit), Ok(()));
        let over_the_limit = "あ".repeat(MEMO_MAX_CHARS + 1);
        assert_eq!(validate(&over_the_limit), Err(MemoError::TooLong));
    }

    #[test]
    fn html_tags_stay_as_text_for_display() {
        let memo = "<b>注意</b><script>alert('x')</script>";
        assert_eq!(validate(memo), Ok(()));
        assert_eq!(display_text(memo), memo);
    }
}
