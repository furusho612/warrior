//! 種目ライブラリの一覧の絞り込み、並び、カーソル方式のページング。
//!
//! 一覧は部位区分、使用器具、種目名の五十音順の昇順で並べる。種目名の比較には
//! `name_kana` を使う。複数の条件を指定したときは、全ての条件を満たす種目だけを残す。
//! カーソルは直前のページの最後の種目の並び順のキーと ID を持つ。

use std::cmp::Ordering;
use std::num::NonZeroUsize;

use serde::{Deserialize, Serialize};

use crate::cursor::{encode_cursor, Cursor};
use crate::exercise::{BodyPart, Equipment, Exercise};

/// 一覧の 1 件。お気に入りの状態を種目と一緒に運ぶ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryExercise {
    #[serde(flatten)]
    pub exercise: Exercise,
    pub favorite: bool,
}

impl LibraryExercise {
    /// 並び順のキー。部位区分、使用器具の順序を 2 桁ずつ前置し、読みの比較用のキーを続ける。
    pub fn sort_key(&self) -> String {
        format!(
            "{:02}{:02}{}",
            self.exercise.body_part.order(),
            self.exercise.equipment.order(),
            kana_sort_key(&self.exercise.name_kana)
        )
    }
}

/// 種目ライブラリの一覧の応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExerciseList {
    pub exercises: Vec<LibraryExercise>,
    /// 続きのページを要求するカーソル。最後のページでは `None`。
    pub next_cursor: Option<String>,
}

/// お気に入りの登録または解除の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoriteState {
    pub favorite: bool,
}

/// 一覧の絞り込み条件。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExerciseFilter {
    /// 部位区分。指定しないときは全ての部位を残す。
    pub body_part: Option<BodyPart>,
    /// 使用器具。指定しないときは全ての器具を残す。
    pub equipment: Option<Equipment>,
    /// お気に入りだけを残すか。
    pub favorite_only: bool,
    /// 種目名と読みの部分一致。前後の空白は無視する。
    pub query: String,
}

impl ExerciseFilter {
    /// 種目が全ての条件を満たすかを返す。
    pub fn matches(&self, item: &LibraryExercise) -> bool {
        if self
            .body_part
            .is_some_and(|part| item.exercise.body_part != part)
        {
            return false;
        }
        if self
            .equipment
            .is_some_and(|equipment| item.exercise.equipment != equipment)
        {
            return false;
        }
        if self.favorite_only && !item.favorite {
            return false;
        }
        let query = self.query.trim();
        query.is_empty()
            || item.exercise.name.contains(query)
            || item.exercise.name_kana.contains(query)
    }
}

/// 条件を満たす種目だけを残す。
pub fn filter_exercises(
    exercises: Vec<LibraryExercise>,
    filter: &ExerciseFilter,
) -> Vec<LibraryExercise> {
    exercises
        .into_iter()
        .filter(|item| filter.matches(item))
        .collect()
}

/// 部位区分、使用器具、読み、ID の順に並べる。
pub fn sort_exercises(exercises: &mut [LibraryExercise]) {
    exercises.sort_by(compare_exercises);
}

/// 並び済みの一覧から 1 ページを取り出す。
///
/// カーソルが指す種目より後ろから `page_size` 件を返す。続きがあるときは、
/// 返した最後の種目の並び順のキーと ID をカーソルにする。
pub fn paginate(
    exercises: &[LibraryExercise],
    cursor: Option<&Cursor>,
    page_size: NonZeroUsize,
) -> ExerciseList {
    let start = cursor.map_or(0, |cursor| {
        exercises.partition_point(|item| {
            let key = item.sort_key();
            (key.as_str(), item.exercise.id.as_str()) <= (cursor.key.as_str(), cursor.id.as_str())
        })
    });
    let end = start.saturating_add(page_size.get()).min(exercises.len());
    let next_cursor = if end < exercises.len() {
        end.checked_sub(1)
            .and_then(|index| exercises.get(index))
            .and_then(|last| encode_cursor(&last.sort_key(), &last.exercise.id))
    } else {
        None
    };
    ExerciseList {
        exercises: exercises[start..end].to_vec(),
        next_cursor,
    }
}

/// 種目の一覧を並べる順序を比較する。
///
/// 長音は直前の母音として比べる。長音をそのままの文字で比べると、
/// 「しー」が「しょ」より後ろになり、五十音順にならないためである。
pub fn compare_exercises(left: &LibraryExercise, right: &LibraryExercise) -> Ordering {
    (
        left.exercise.body_part.order(),
        left.exercise.equipment.order(),
        kana_sort_key(&left.exercise.name_kana),
        left.exercise.id.as_str(),
    )
        .cmp(&(
            right.exercise.body_part.order(),
            right.exercise.equipment.order(),
            kana_sort_key(&right.exercise.name_kana),
            right.exercise.id.as_str(),
        ))
}

/// 読みを五十音順に比べるためのキーを作る。
///
/// 長音を直前の母音に置き換える。母音を取れない位置の長音はそのまま残す。
pub fn kana_sort_key(text: &str) -> String {
    let mut key = String::new();
    let mut previous = None;
    for character in text.chars() {
        if character == 'ー' {
            key.push(previous.and_then(long_vowel).unwrap_or(character));
        } else {
            key.push(character);
        }
        previous = Some(character);
    }
    key
}

fn long_vowel(character: char) -> Option<char> {
    match character {
        'あ' | 'か' | 'が' | 'さ' | 'ざ' | 'た' | 'だ' | 'な' | 'は' | 'ば' | 'ぱ' | 'ま'
        | 'や' | 'ら' | 'わ' | 'ぁ' | 'ゃ' => Some('あ'),
        'い' | 'き' | 'ぎ' | 'し' | 'じ' | 'ち' | 'ぢ' | 'に' | 'ひ' | 'び' | 'ぴ' | 'み'
        | 'り' | 'ぃ' => Some('い'),
        'う' | 'く' | 'ぐ' | 'す' | 'ず' | 'つ' | 'づ' | 'ぬ' | 'ふ' | 'ぶ' | 'ぷ' | 'む'
        | 'ゆ' | 'る' | 'ぅ' | 'ゅ' | 'ゔ' => Some('う'),
        'え' | 'け' | 'げ' | 'せ' | 'ぜ' | 'て' | 'で' | 'ね' | 'へ' | 'べ' | 'ぺ' | 'め'
        | 'れ' | 'ぇ' => Some('え'),
        'お' | 'こ' | 'ご' | 'そ' | 'ぞ' | 'と' | 'ど' | 'の' | 'ほ' | 'ぼ' | 'ぽ' | 'も'
        | 'よ' | 'ろ' | 'を' | 'ぉ' | 'ょ' => Some('お'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::kana_sort_key;

    #[test]
    fn the_kana_sort_key_treats_a_long_vowel_as_the_previous_vowel() {
        assert_eq!(kana_sort_key("しーてっどろう"), "しいてっどろう");
        assert_eq!(kana_sort_key("けーぶる"), "けえぶる");
        assert!(kana_sort_key("しーてっど") < kana_sort_key("しょるだー"));
    }
}
