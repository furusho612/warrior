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
    PasskeyTitle => "パスキー",
    PasskeyDescription => "登録したパスキーでログインできます。",
    PasskeyAdd => "パスキーを追加",
    PasskeyDelete => "削除",
    PasskeyEmpty => "パスキーはまだ登録されていません。",
    PasskeyNeedsReauth => "操作の前に Google アカウントまたはパスキーで再認証してください。",
    PasskeyGoogleReauth => "Google で再認証",
    PasskeyPasskeyReauth => "パスキーで再認証",
    PasskeyRegistered => "パスキーを登録しました。",
    PasskeyDeleted => "パスキーを削除しました。",
    PasskeyFailed => "パスキーの操作に失敗しました。",
    PasskeyReauthenticated => "再認証しました。",
    PwaUpdateAvailable => "新しい版があります。",
    PwaReload => "再読み込み",
    IosInstallHint => "iPhone で通知を受け取るには、共有メニューからホーム画面に追加してください。",
    PasskeyRecovery => "Google アカウントとすべてのパスキーを失うと、アカウントは復旧できません。",
    PasskeyLoginTitle => "パスキーでログイン",
    PasskeyEmailLabel => "Google アカウントのメールアドレス",
    PasskeyEmailPlaceholder => "you@example.com",
    PasskeyLoginButton => "パスキーでログイン",
    PasskeyDiscoverableButton => "端末のパスキーでログイン",
    NavHome => "ホーム",
    NavCalendar => "カレンダー",
    NavLibrary => "種目",
    NavAnalysis => "分析",
    NavMypage => "マイページ",
    HomeTitle => "ホーム",
    HomeDescription => "今日のメニューと部位ごとの回復状況をここに表示します。",
    HomeStart => "トレーニングを開始",
    RecordTitle => "記録中",
    RecordDescription => "セットの入力とレストタイマーをここに表示します。",
    RecordStart => "記録を開始",
    RecordMemoLabel => "メモ",
    RecordMemoPlaceholder => "トレーニングのメモ",
    RecordLoading => "読み込み中",
    RecordSaving => "保存中",
    RecordLoadFailed => "記録を読み込めませんでした。",
    RecordSaveFailed => "端末への保存に失敗しました。",
    RecordRetry => "再試行",
    RecordDiscard => "破棄",
    RecordStartFailed => "トレーニングを開始できませんでした。",
    RecordFinish => "終了",
    RecordFinishing => "終了しています",
    RecordFinishFailed => "トレーニングを終了できませんでした。通信を確認して、もう一度お試しください。",
    RecordPendingOperations => "送信中の操作があります。通信の復帰を待ってください",
    RecordFailedOperationsRemain => "失敗した操作を再試行するか破棄してください",
    RecordSets => "セット",
    RecordSetDetail => "セット詳細",
    RecordSetDetailEmpty => "セットを選ぶと詳細を表示します。",
    RecordFailedOperations => "送信に失敗した操作",
    RecordOperationCreate => "トレーニングの作成",
    RecordOperationUpdate => "トレーニングの更新",
    RecordViewOnly => "ほかの端末で記録中です。この端末では閲覧のみできます。",
    RecordTakeover => "引き継ぐ",
    RecordTakeoverConfirm => "引き継ぐと、元の端末にだけある未保存の変更と、この端末の未保存の変更は失われます。引き継ぎますか。",
    RecordTakeoverAccept => "引き継ぎを実行する",
    RecordTakeoverCancel => "キャンセル",
    RecordTakeoverFailed => "記録を引き継げませんでした。通信を確認して、もう一度お試しください。",
    RecordTakeoverFinished => "終了したトレーニングは引き継げません。",
    RecordReloadFromServer => "サーバーの内容を読み直す",
    RecordReloadFailed => "サーバーの内容を読み込めませんでした。",
    SyncSaved => "保存済み",
    SyncUnsaved => "未保存",
    SyncFailed => "送信に失敗",
    RecordRail => "種目",
    RestTimer => "レストタイマー",
    CalendarTitle => "カレンダー",
    CalendarPreviousMonth => "前の月",
    CalendarNextMonth => "次の月",
    CalendarTrainingDays => "トレーニング",
    CalendarDaysUnit => "日",
    CalendarParts => "部位",
    CalendarAllParts => "すべて",
    CalendarPartChest => "胸",
    CalendarPartBack => "背中",
    CalendarPartShoulders => "肩",
    CalendarPartBicepsForearm => "二頭・前腕",
    CalendarPartTriceps => "三頭",
    CalendarPartLegs => "脚",
    CalendarPartAbs => "腹筋",
    CalendarPartOther => "その他",
    CalendarWeekdayMonday => "月",
    CalendarWeekdayTuesday => "火",
    CalendarWeekdayWednesday => "水",
    CalendarWeekdayThursday => "木",
    CalendarWeekdayFriday => "金",
    CalendarWeekdaySaturday => "土",
    CalendarWeekdaySunday => "日",
    CalendarLoading => "読み込み中",
    CalendarLoadFailed => "カレンダーを読み込めませんでした。",
    CalendarDayLoadFailed => "この日の記録を読み込めませんでした。",
    CalendarSelectDay => "日付を選ぶと、その日の記録を表示します。",
    CalendarNoTraining => "この日のトレーニングはありません。",
    CalendarTotalWeight => "総重量",
    CalendarSetKindNormal => "通常",
    CalendarSetKindWarmup => "ウォームアップ",
    CalendarSetKindDrop => "ドロップ",
    CalendarGripPronated => "順手",
    CalendarGripSupinated => "逆手",
    CalendarGripParallel => "パラレル",
    CalendarGripWide => "ワイド",
    CalendarGripMedium => "ミディアム",
    CalendarGripNarrow => "ナロー",
    CalendarAttachmentWideBar => "ワイドバー",
    CalendarAttachmentStraightBar => "ストレートバー",
    CalendarAttachmentParallelGrip => "パラレルグリップ",
    CalendarAttachmentMagNarrowParallel => "マグナローパラレル",
    CalendarAttachmentMagNarrowOver => "マグナローオーバー",
    CalendarAttachmentMagNarrowUnder => "マグナローアンダー",
    CalendarAttachmentMagMediumParallel => "マグミディアムパラレル",
    CalendarAttachmentMagMediumOver => "マグミディアムオーバー",
    CalendarAttachmentMagMediumUnder => "マグミディアムアンダー",
    CalendarAttachmentMagWideOver => "マグワイドオーバー",
    CalendarMomentum => "反動",
    CalendarAssisted => "補助",
    CalendarLeft => "左",
    CalendarRight => "右",
    CalendarReps => "回",
    CalendarSeconds => "秒",
    CalendarMinutes => "分",
    CalendarRetry => "再試行",
    LibraryTitle => "種目ライブラリ",
    LibraryDescription => "種目の検索と絞り込みをここに表示します。",
    ExerciseTitle => "種目詳細",
    ExerciseDescription => "種目の自己ベストと履歴をここに表示します。",
    AnalysisTitle => "分析",
    AnalysisDescription => "部位別のセット数と回復状況をここに表示します。",
    SummaryTitle => "終了サマリー",
    SummaryLoading => "読み込み中",
    SummaryLoadFailed => "サマリーを読み込めませんでした。",
    SummaryDuration => "所要時間",
    SummaryTotalWeight => "総重量",
    SummaryExerciseCount => "種目数",
    SummarySetCount => "セット数",
    SummaryPersonalBests => "更新した自己ベスト",
    SummaryNoPersonalBests => "更新した自己ベストはありません。",
    SummaryBestMaxWeight => "最大重量",
    SummaryBestEstimatedOneRepMax => "推定 1RM",
    SummaryBestTotalWeight => "種目別の総重量",
    SummaryBestLongestDuration => "最長時間",
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
