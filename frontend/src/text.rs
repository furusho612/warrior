//! 画面に表示する文言のキーと日本語の表。
//!
//! 画面のコードに文字列を直接書かず、この表から引く。
//! i18n の汎用クレートは追加しない (ADR-025)。

macro_rules! text_keys {
    ($($variant:ident => $value:expr),* $(,)?) => {
        /// 画面の文言のキー。
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum Key {
            $($variant,)*
        }

        /// 全てのキー。
        pub const ALL_KEYS: &[Key] = &[$(Key::$variant,)*];

        /// キーに対応する日本語の文言。
        pub fn text(key: Key) -> &'static str {
            match key {
                $(Key::$variant => $value,)*
            }
        }
    };
}

text_keys! {
    AppName => "WARRIOR",
    LoginTitle => "ログイン",
    LoginDescription => "Google アカウントでログインすると、記録を複数の端末から見られます。",
    LoginButton => "Google でログイン",
    LoginError => "ログインできませんでした。もう一度お試しください。",
    LogoutButton => "ログアウト",
    NavHome => "ホーム",
    NavCalendar => "カレンダー",
    NavLibrary => "種目",
    NavAnalysis => "分析",
    NavMypage => "マイページ",
    HomeTitle => "ホーム",
    HomeDescription => "今日のメニューと部位ごとの回復状況をここに表示します。",
    RecordTitle => "記録中",
    RecordDescription => "セットの入力とレストタイマーをここに表示します。",
    RecordRail => "種目",
    RestTimer => "レストタイマー",
    CalendarTitle => "カレンダー",
    CalendarDescription => "日ごとのトレーニングと部位をここに表示します。",
    LibraryTitle => "種目ライブラリ",
    LibraryDescription => "種目の検索と絞り込みをここに表示します。",
    ExerciseTitle => "種目詳細",
    ExerciseDescription => "種目の自己ベストと履歴をここに表示します。",
    AnalysisTitle => "分析",
    AnalysisDescription => "部位別のセット数と回復状況をここに表示します。",
    SummaryTitle => "終了サマリー",
    SummaryDescription => "所要時間、総重量、自己ベスト、AI の評価をここに表示します。",
    RoutinesTitle => "ルーティンとプログラム",
    RoutinesDescription => "進行中のプログラムとルーティンをここに表示します。",
    MyPageTitle => "マイページ",
    MyPageDescription => "プロフィール、栄養、表示、データ、アカウントの設定をここに表示します。",
    NotFoundTitle => "画面が見つかりません",
    NotFoundDescription => "ホームへ戻って操作を続けてください。",
    AiMedicalDisclaimer => "AI の評価と提案は医療上の助言ではありません。持病がある場合や妊娠中の場合は、医師などの専門家に相談してください。",
    AiSafetyAdvice => "負荷を下げるか休んでください。記述が続く場合は、医師などの専門家に相談してください。",
}

#[cfg(test)]
mod tests {
    use super::{text, Key, ALL_KEYS};

    #[test]
    fn every_key_has_non_empty_text() {
        for key in ALL_KEYS {
            assert!(!text(*key).trim().is_empty(), "{key:?} must have text");
        }
    }

    #[test]
    fn navigation_labels_are_japanese() {
        assert_eq!(text(Key::NavHome), "ホーム");
        assert_eq!(text(Key::NavCalendar), "カレンダー");
        assert_eq!(text(Key::NavLibrary), "種目");
        assert_eq!(text(Key::NavAnalysis), "分析");
        assert_eq!(text(Key::NavMypage), "マイページ");
    }

    #[test]
    fn every_key_appears_once_in_the_table() {
        let mut keys = ALL_KEYS.to_vec();
        keys.sort_by_key(|key| format!("{key:?}"));
        keys.dedup();
        assert_eq!(keys.len(), ALL_KEYS.len());
    }
}
