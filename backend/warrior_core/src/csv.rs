//! 記録の CSV の列、セルの符号化と復号、セットの 1 行の組み立て (REQ-MYP-008)。
//!
//! 1 行を 1 セットとし、列は [`COLUMNS`] の順に固定する。ドロップセットは段を `;` で、
//! 左右別のセットは左右を `/` で区切って 1 行にまとめる。
//! セルはカンマ、引用符、改行を含むときに引用符で囲み、`=`、`+`、`-`、`@` で始まるセルは
//! 先頭に `'` を付けて表計算ソフトが数式として実行しないようにする。
//! 文書の先頭には UTF-8 の BOM を付ける。
//!
//! セットの計算は `set_calc` の値を使い、この module は値の表記だけを担う。

use crate::exercise::{Attachment, BodyPart, GripSelection, GripStyle, GripWidth};
use crate::rpe;
use crate::set_calc::{RecordMode, SetKind, SetRecord, SetSegment, SideValue};

/// CSV の列の名前。この順に 1 行を組み立てる。
pub const COLUMNS: [&str; 12] = [
    "日付",
    "種目",
    "部位区分",
    "セット番号",
    "セット種別",
    "重量",
    "回数",
    "時間",
    "RPE",
    "反動と補助の回数",
    "グリップとアタッチメント",
    "メモ",
];

/// 文書の先頭に付ける UTF-8 の BOM。
pub const UTF8_BOM: &str = "\u{feff}";

/// CSV の行の区切り。RFC 4180 と同じ改行にする。
const ROW_SEPARATOR: &str = "\r\n";

/// 段の値を区切る文字。
const SEGMENT_SEPARATOR: char = ';';

/// 左右の値を区切る文字。
const SIDE_SEPARATOR: char = '/';

/// メモの `;` と `\` をエスケープする文字。
const MEMO_ESCAPE: char = '\\';

/// CSV の 1 行にするセットの記録。
#[derive(Debug, Clone, PartialEq)]
pub struct CsvSet {
    /// トレーニングの日付 (日本時間の `YYYY-MM-DD`)。
    pub date: String,
    /// 記録した時点の種目の名前。
    pub exercise_name: String,
    /// 記録した時点の部位区分。
    pub body_part: BodyPart,
    /// 種目の中のセット番号。1 から始まる。
    pub set_number: u32,
    /// セットの記録。
    pub record: SetRecord,
    /// 記録した時点のグリップまたはアタッチメント。
    pub grip: GripSelection,
    /// トレーニングと種目のメモを [`join_memos`] でまとめたセル。
    pub memo: String,
}

/// 列の名前をセルの列にする。
pub fn column_names() -> Vec<String> {
    COLUMNS.iter().map(|column| (*column).to_owned()).collect()
}

/// セルを 1 つの表計算ソフト向けの値にする。数式として解釈される先頭文字を無害化する。
pub fn encode_cell(cell: &str) -> String {
    quote_cell(&neutralize_cell(cell))
}

/// 数式として解釈される先頭文字に `'` を付ける。
pub fn neutralize_cell(cell: &str) -> String {
    match cell.chars().next() {
        Some('=' | '+' | '-' | '@') => format!("'{cell}"),
        _ => cell.to_owned(),
    }
}

/// カンマ、引用符、改行を含むセルを引用符で囲み、内側の引用符を 2 つ重ねる。
pub fn quote_cell(cell: &str) -> String {
    if !cell.contains([',', '"', '\r', '\n']) {
        return cell.to_owned();
    }
    format!("\"{}\"", cell.replace('"', "\"\""))
}

/// セルの列を 1 行にする。
pub fn encode_row(cells: &[String]) -> String {
    cells
        .iter()
        .map(|cell| encode_cell(cell))
        .collect::<Vec<_>>()
        .join(",")
}

/// セットの列を、BOM と列の名前を付けた CSV の文書にする。
pub fn encode_document(sets: &[CsvSet]) -> Vec<u8> {
    let mut document = String::from(UTF8_BOM);
    document.push_str(&encode_row(&column_names()));
    document.push_str(ROW_SEPARATOR);
    for set in sets {
        document.push_str(&encode_row(&set_cells(set)));
        document.push_str(ROW_SEPARATOR);
    }
    document.into_bytes()
}

/// CSV の文書を、行ごとのセルに戻す。列の数と引用の整合が取れない文書は受け付けない。
pub fn parse_document(text: &str) -> Option<Vec<Vec<String>>> {
    let text = text.strip_prefix(UTF8_BOM).unwrap_or(text);
    parse_records(text)
}

/// CSV の 1 行を、セルの列に戻す。
///
/// セルの中の改行は引用符で囲まれているため、行の区切りとして扱わない。
pub fn parse_row(row: &str) -> Option<Vec<String>> {
    if row.is_empty() {
        return Some(vec![String::new()]);
    }
    let mut records = parse_records(row)?;
    if records.len() != 1 {
        return None;
    }
    records.pop()
}

/// トレーニングと種目のメモを 1 つのセルにする。空のメモは省く。
pub fn join_memos(training_memo: &str, exercise_memo: &str) -> String {
    [training_memo, exercise_memo]
        .into_iter()
        .filter(|memo| !memo.is_empty())
        .map(escape_memo)
        .collect::<Vec<_>>()
        .join(&SEGMENT_SEPARATOR.to_string())
}

/// メモのセルを、トレーニングと種目のメモの列に戻す。空のセルは空の列にする。
pub fn split_memos(cell: &str) -> Vec<String> {
    if cell.is_empty() {
        return Vec::new();
    }
    let mut memos = Vec::new();
    let mut current = String::new();
    let mut characters = cell.chars();
    while let Some(character) = characters.next() {
        match character {
            MEMO_ESCAPE => {
                if let Some(escaped) = characters.next() {
                    current.push(escaped);
                }
            }
            SEGMENT_SEPARATOR => memos.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    memos.push(current);
    memos
}

fn escape_memo(memo: &str) -> String {
    let mut escaped = String::with_capacity(memo.len());
    for character in memo.chars() {
        if character == MEMO_ESCAPE || character == SEGMENT_SEPARATOR {
            escaped.push(MEMO_ESCAPE);
        }
        escaped.push(character);
    }
    escaped
}

fn set_cells(set: &CsvSet) -> [String; 12] {
    [
        set.date.clone(),
        set.exercise_name.clone(),
        body_part_label(set.body_part).to_owned(),
        set.set_number.to_string(),
        set_kind_label(set.record.kind()).to_owned(),
        weight_cell(&set.record),
        reps_cell(&set.record),
        duration_cell(&set.record),
        set.record.rpe().map(rpe::format).unwrap_or_default(),
        momentum_and_assisted_cell(&set.record),
        grip_label(set.grip),
        set.memo.clone(),
    ]
}

fn weight_cell(record: &SetRecord) -> String {
    if record.mode() == RecordMode::Time {
        return String::new();
    }
    join_segments(record.segments(), |segment| {
        if segment.left.is_some() || segment.right.is_some() {
            format_sides(segment, |side| format_weight(side.weight_kg))
        } else {
            format_weight(segment.weight_kg)
        }
    })
}

fn reps_cell(record: &SetRecord) -> String {
    if record.mode() == RecordMode::Time {
        return String::new();
    }
    join_segments(record.segments(), |segment| {
        if segment.left.is_some() || segment.right.is_some() {
            format_sides(segment, |side| side.reps.to_string())
        } else {
            segment.reps.to_string()
        }
    })
}

fn duration_cell(record: &SetRecord) -> String {
    if record.mode() != RecordMode::Time {
        return String::new();
    }
    if record.segments().is_empty() {
        return record
            .duration_seconds()
            .map(|duration| duration.to_string())
            .unwrap_or_default();
    }
    join_segments(record.segments(), |segment| {
        if segment.left.is_some() || segment.right.is_some() {
            format_sides(segment, |side| side.duration_seconds().to_string())
        } else {
            segment
                .duration_seconds
                .map(|duration| duration.to_string())
                .unwrap_or_default()
        }
    })
}

fn momentum_and_assisted_cell(record: &SetRecord) -> String {
    let Some(segment) = record.segments().first() else {
        return "0/0".to_owned();
    };
    format!("{}/{}", segment.momentum_reps, segment.assisted_reps)
}

/// 左右の値を `/` で区切る。片側だけを持つ段は空の側を空にする。
fn format_sides(segment: &SetSegment, value: impl Fn(&SideValue) -> String) -> String {
    let left = segment.left.as_ref().map(&value).unwrap_or_default();
    let right = segment.right.as_ref().map(value).unwrap_or_default();
    format!("{left}{SIDE_SEPARATOR}{right}")
}

fn join_segments(segments: &[SetSegment], cell: impl Fn(&SetSegment) -> String) -> String {
    segments
        .iter()
        .map(cell)
        .collect::<Vec<_>>()
        .join(&SEGMENT_SEPARATOR.to_string())
}

fn format_weight(value: f64) -> String {
    format!("{value}")
}

fn body_part_label(body_part: BodyPart) -> &'static str {
    match body_part {
        BodyPart::Chest => "胸",
        BodyPart::Back => "背中",
        BodyPart::Shoulders => "肩",
        BodyPart::BicepsForearm => "二頭・前腕",
        BodyPart::Triceps => "三頭",
        BodyPart::Legs => "脚",
        BodyPart::Abs => "腹筋",
        BodyPart::Other => "その他",
    }
}

fn set_kind_label(kind: SetKind) -> &'static str {
    match kind {
        SetKind::Normal => "通常",
        SetKind::Warmup => "ウォームアップ",
        SetKind::Drop => "ドロップセット",
    }
}

fn grip_label(grip: GripSelection) -> String {
    match grip {
        GripSelection::None => String::new(),
        GripSelection::Grip { style, width } => {
            format!("{} {}", grip_style_label(style), grip_width_label(width))
        }
        GripSelection::Attachment { attachment, style } => match style {
            Some(style) => format!(
                "{} / {}",
                grip_style_label(style),
                attachment_label(attachment)
            ),
            None => attachment_label(attachment).to_owned(),
        },
    }
}

fn grip_style_label(style: GripStyle) -> &'static str {
    match style {
        GripStyle::Pronated => "順手",
        GripStyle::Supinated => "逆手",
        GripStyle::Parallel => "パラレル",
    }
}

fn grip_width_label(width: GripWidth) -> &'static str {
    match width {
        GripWidth::Wide => "ワイド",
        GripWidth::Medium => "ミディアム",
        GripWidth::Narrow => "ナロー",
    }
}

fn attachment_label(attachment: Attachment) -> &'static str {
    match attachment {
        Attachment::WideBar => "ワイドバー",
        Attachment::StraightBar => "ストレートバー",
        Attachment::ParallelGrip => "パラレルグリップ",
        Attachment::MagNarrowParallel => "マグナローパラレル",
        Attachment::MagNarrowOver => "マグナローオーバー",
        Attachment::MagNarrowUnder => "マグナローアンダー",
        Attachment::MagMediumParallel => "マグミディアムパラレル",
        Attachment::MagMediumOver => "マグミディアムオーバー",
        Attachment::MagMediumUnder => "マグミディアムアンダー",
        Attachment::MagWideOver => "マグワイドオーバー",
    }
}

/// 引用符と区切りの状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// セルの先頭。
    FieldStart,
    /// 引用符で囲まれていないセルの中。
    Unquoted,
    /// 引用符で囲まれたセルの中。
    Quoted,
    /// 引用符で囲まれたセルを閉じた直後。
    QuoteEnd,
}

fn parse_records(text: &str) -> Option<Vec<Vec<String>>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = State::FieldStart;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if !matches!(state, State::Quoted) {
            match character {
                ',' => {
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    continue;
                }
                '\r' | '\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = State::FieldStart;
                    if character == '\r' && characters.peek() == Some(&'\n') {
                        characters.next();
                    }
                    continue;
                }
                _ => {}
            }
        }
        match state {
            State::Quoted => {
                if character != '"' {
                    field.push(character);
                } else if characters.peek() == Some(&'"') {
                    characters.next();
                    field.push('"');
                } else {
                    state = State::QuoteEnd;
                }
            }
            State::QuoteEnd => return None,
            State::FieldStart => {
                if character == '"' {
                    state = State::Quoted;
                } else {
                    field.push(character);
                    state = State::Unquoted;
                }
            }
            State::Unquoted => {
                if character == '"' {
                    return None;
                }
                field.push(character);
            }
        }
    }
    if matches!(state, State::Quoted) {
        return None;
    }
    if !matches!(state, State::FieldStart) || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Some(records)
}

#[cfg(test)]
mod tests {
    //! CSV のセルの符号化と、セットの 1 行の例示テスト。
    //!
    //! 対応する要求: `REQ-MYP-008`。任意のセルの往復と行数は性質テストが担う。

    use crate::exercise::{Attachment, BodyPart, GripSelection, GripStyle, GripWidth};
    use crate::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue};

    use super::{
        encode_cell, encode_document, encode_row, join_memos, parse_document, parse_row, set_cells,
        split_memos, CsvSet, COLUMNS, UTF8_BOM,
    };

    #[test]
    fn req_myp_008_the_columns_are_fixed_in_the_documented_order() {
        assert_eq!(
            COLUMNS,
            [
                "日付",
                "種目",
                "部位区分",
                "セット番号",
                "セット種別",
                "重量",
                "回数",
                "時間",
                "RPE",
                "反動と補助の回数",
                "グリップとアタッチメント",
                "メモ",
            ]
        );
    }

    #[test]
    fn req_myp_008_a_cell_is_quoted_for_commas_quotes_and_newlines() {
        assert_eq!(encode_cell("ベンチプレス"), "ベンチプレス");
        assert_eq!(encode_cell("a,b"), "\"a,b\"");
        assert_eq!(encode_cell("a\"b"), "\"a\"\"b\"");
        assert_eq!(encode_cell("a\nb"), "\"a\nb\"");
        assert_eq!(encode_cell("a\r\nb"), "\"a\r\nb\"");
        assert_eq!(encode_cell(""), "");
    }

    #[test]
    fn req_myp_008_a_formula_cell_is_neutralized_before_quoting() {
        for cell in ["=SUM(A1)", "+1", "-1", "@cmd"] {
            assert_eq!(encode_cell(cell), format!("'{cell}"));
        }
        assert_eq!(encode_cell("=a,b"), "\"'=a,b\"");
        assert_eq!(encode_cell("a=b"), "a=b");
        assert_eq!(encode_cell("'=a"), "'=a");
    }

    #[test]
    fn req_myp_008_a_row_round_trips_through_the_parser() {
        let cells = vec![
            "ベンチプレス".to_owned(),
            "a,b".to_owned(),
            "a\"b".to_owned(),
            "a\nb".to_owned(),
            String::new(),
        ];
        assert_eq!(parse_row(&encode_row(&cells)), Some(cells));
        assert_eq!(
            parse_row(&encode_row(&[String::new()])),
            Some(vec![String::new()]),
            "a row with only an empty cell round trips"
        );
    }

    #[test]
    fn req_myp_008_the_document_starts_with_a_bom_and_one_row_per_set() {
        let document = encode_document(&[set(CsvSetFields::weight(), 1)]);
        assert!(document.starts_with(b"\xEF\xBB\xBF"));
        let text = String::from_utf8(document).expect("the document is UTF-8");
        let rows = parse_document(&text).expect("the document is CSV");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], COLUMNS);
        assert_eq!(rows[1].len(), COLUMNS.len());
        assert!(text.starts_with(UTF8_BOM));
    }

    #[test]
    fn req_myp_008_a_drop_set_joins_the_segments_with_a_semicolon() {
        let cells = set_cells(&set(
            CsvSetFields {
                kind: SetKind::Drop,
                segments: vec![segment(100.0, 8), segment(80.0, 6)],
                ..CsvSetFields::weight()
            },
            2,
        ));
        assert_eq!(cells[4], "ドロップセット");
        assert_eq!(cells[5], "100;80");
        assert_eq!(cells[6], "8;6");
        assert_eq!(cells[7], "");
    }

    #[test]
    fn req_myp_008_a_unilateral_set_joins_the_sides_with_a_slash() {
        let cells = set_cells(&set(
            CsvSetFields {
                segments: vec![SideSegment {
                    left: Some(SideValue {
                        weight_kg: 10.0,
                        reps: 5,
                        duration_seconds: None,
                    }),
                    right: Some(SideValue {
                        weight_kg: 12.5,
                        reps: 4,
                        duration_seconds: None,
                    }),
                    momentum_reps: 2,
                    assisted_reps: 1,
                }
                .into()],
                ..CsvSetFields::weight()
            },
            1,
        ));
        assert_eq!(cells[5], "10/12.5");
        assert_eq!(cells[6], "5/4");
        assert_eq!(cells[9], "2/1");
    }

    #[test]
    fn req_myp_008_a_unilateral_drop_set_joins_the_segments_and_the_sides() {
        let cells = set_cells(&set(
            CsvSetFields {
                kind: SetKind::Drop,
                segments: vec![
                    SideSegment {
                        left: Some(SideValue {
                            weight_kg: 10.0,
                            reps: 5,
                            duration_seconds: None,
                        }),
                        right: Some(SideValue {
                            weight_kg: 12.5,
                            reps: 4,
                            duration_seconds: None,
                        }),
                        momentum_reps: 0,
                        assisted_reps: 0,
                    }
                    .into(),
                    SideSegment {
                        left: Some(SideValue {
                            weight_kg: 8.0,
                            reps: 6,
                            duration_seconds: None,
                        }),
                        right: Some(SideValue {
                            weight_kg: 9.0,
                            reps: 5,
                            duration_seconds: None,
                        }),
                        momentum_reps: 0,
                        assisted_reps: 0,
                    }
                    .into(),
                ],
                ..CsvSetFields::weight()
            },
            1,
        ));
        assert_eq!(cells[5], "10/12.5;8/9");
        assert_eq!(cells[6], "5/4;6/5");
    }

    #[test]
    fn req_myp_008_a_time_set_writes_the_seconds_and_empty_weight_and_reps() {
        let cells = set_cells(&set(
            CsvSetFields {
                mode: RecordMode::Time,
                segments: vec![time_segment(90)],
                ..CsvSetFields::weight()
            },
            1,
        ));
        assert_eq!(cells[5], "");
        assert_eq!(cells[6], "");
        assert_eq!(cells[7], "90");
    }

    #[test]
    fn req_myp_008_a_unilateral_time_set_joins_the_sides() {
        let cells = set_cells(&set(
            CsvSetFields {
                mode: RecordMode::Time,
                segments: vec![time_segment_with_sides(30, 60)],
                ..CsvSetFields::weight()
            },
            1,
        ));
        assert_eq!(cells[7], "30/60");
    }

    #[test]
    fn req_myp_008_the_rpe_and_the_grip_are_written_as_japanese_labels() {
        let cells = set_cells(&set(
            CsvSetFields {
                rpe: Some(8.5),
                grip: GripSelection::Grip {
                    style: GripStyle::Pronated,
                    width: GripWidth::Wide,
                },
                ..CsvSetFields::weight()
            },
            1,
        ));
        assert_eq!(cells[8], "8.5");
        assert_eq!(cells[10], "順手 ワイド");

        let empty = set_cells(&set(
            CsvSetFields {
                rpe: None,
                grip: GripSelection::None,
                ..CsvSetFields::weight()
            },
            1,
        ));
        assert_eq!(empty[8], "");
        assert_eq!(empty[10], "");

        let attachment = set_cells(&set(
            CsvSetFields {
                grip: GripSelection::Attachment {
                    attachment: Attachment::WideBar,
                    style: Some(GripStyle::Supinated),
                },
                ..CsvSetFields::weight()
            },
            1,
        ));
        assert_eq!(attachment[10], "逆手 / ワイドバー");
    }

    #[test]
    fn req_myp_008_the_body_part_is_a_japanese_label() {
        let cells = set_cells(&set(CsvSetFields::weight(), 1));
        assert_eq!(cells[2], "胸");
    }

    #[test]
    fn req_myp_008_the_memos_are_escaped_and_joined_with_a_semicolon() {
        assert_eq!(join_memos("腰が痛い", "フォーム"), "腰が痛い;フォーム");
        assert_eq!(join_memos("", "フォーム"), "フォーム");
        assert_eq!(join_memos("腰が痛い", ""), "腰が痛い");
        assert_eq!(join_memos("", ""), "");
        assert_eq!(
            join_memos("a;b", "c\\d"),
            "a\\;b;c\\\\d",
            "the separators in the memos must be escaped"
        );
        assert_eq!(split_memos("a\\;b;c\\\\d"), vec!["a;b", "c\\d"]);
        assert_eq!(split_memos(""), Vec::<String>::new());
    }

    struct SideSegment {
        left: Option<SideValue>,
        right: Option<SideValue>,
        momentum_reps: u32,
        assisted_reps: u32,
    }

    impl From<SideSegment> for SetSegment {
        fn from(segment: SideSegment) -> Self {
            Self {
                weight_kg: 0.0,
                reps: 0,
                momentum_reps: segment.momentum_reps,
                assisted_reps: segment.assisted_reps,
                left: segment.left,
                right: segment.right,
                duration_seconds: None,
            }
        }
    }

    fn segment(weight_kg: f64, reps: u32) -> SetSegment {
        SetSegment {
            weight_kg,
            reps,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: None,
        }
    }

    fn time_segment(duration_seconds: u32) -> SetSegment {
        SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: Some(duration_seconds),
        }
    }

    fn time_segment_with_sides(left_seconds: u32, right_seconds: u32) -> SetSegment {
        SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: Some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(left_seconds),
            }),
            right: Some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(right_seconds),
            }),
            duration_seconds: None,
        }
    }

    struct CsvSetFields {
        kind: SetKind,
        mode: RecordMode,
        segments: Vec<SetSegment>,
        rpe: Option<f64>,
        grip: GripSelection,
    }

    impl CsvSetFields {
        fn weight() -> Self {
            Self {
                kind: SetKind::Normal,
                mode: RecordMode::WeightAndReps,
                segments: vec![segment(100.0, 5)],
                rpe: None,
                grip: GripSelection::None,
            }
        }
    }

    fn set(fields: CsvSetFields, set_number: u32) -> CsvSet {
        let record = SetRecord::new(
            BestKey {
                exercise_id: "exercise-1".to_owned(),
                grip: None,
                attachment: None,
            },
            fields.kind,
            fields.segments,
            fields.rpe,
            None,
            fields.mode,
        )
        .expect("the test set is valid");
        CsvSet {
            date: "2026-11-12".to_owned(),
            exercise_name: "ベンチプレス".to_owned(),
            body_part: BodyPart::Chest,
            set_number,
            record,
            grip: fields.grip,
            memo: join_memos("腰が痛い", "フォーム"),
        }
    }
}
