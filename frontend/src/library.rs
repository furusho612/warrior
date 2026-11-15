//! 種目ライブラリの API と、使用器具の表示の文言。
//!
//! 記録への種目の選択は全件を取る。ライブラリの一覧は 1 ページずつ取り、
//! 絞り込みの条件とカーソルをクエリに載せる。

use warrior_core::alternative::RecordedExercises;
use warrior_core::exercise::{BodyPart, Equipment, Exercise};
use warrior_core::library::{ExerciseList, FavoriteState, LibraryExercise};

use crate::http::{self, HttpError};
use crate::storage;
use crate::text::{text, Key};

/// 種目ライブラリの一覧の絞り込み条件。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExercisesQuery {
    pub body_part: Option<BodyPart>,
    pub equipment: Option<Equipment>,
    pub favorite_only: bool,
    pub query: String,
}

impl ExercisesQuery {
    /// 1 ページの要求の URL を組み立てる。
    pub fn page_url(&self, cursor: Option<&str>) -> String {
        let mut params = vec!["paged=true".to_owned()];
        if let Some(part) = self.body_part {
            params.push(format!("body_part={}", part.as_str()));
        }
        if let Some(equipment) = self.equipment {
            params.push(format!("equipment={}", equipment.as_str()));
        }
        if self.favorite_only {
            params.push("favorite=true".to_owned());
        }
        let query = self.query.trim();
        if !query.is_empty() {
            params.push(format!("q={}", encode_query(query)));
        }
        if let Some(cursor) = cursor {
            params.push(format!("cursor={}", encode_query(cursor)));
        }
        format!("/api/exercises?{}", params.join("&"))
    }
}

/// 記録への種目の選択が使う全件の一覧を取る。
pub async fn all() -> Result<ExerciseList, HttpError> {
    http::request_json("GET", "/api/exercises").await
}

/// 種目ライブラリの 1 ページを取る。
pub async fn page(query: &ExercisesQuery, cursor: Option<&str>) -> Result<ExerciseList, HttpError> {
    http::request_json("GET", &query.page_url(cursor)).await
}

/// 種目の詳細を取る。
pub async fn exercise(exercise_id: &str) -> Result<LibraryExercise, HttpError> {
    http::request_json("GET", &format!("/api/exercises/{exercise_id}")).await
}

/// 過去に記録した種目の ID を取る。
pub async fn recorded_exercises() -> Result<RecordedExercises, HttpError> {
    http::request_json("GET", "/api/exercises/recorded").await
}

/// 代わりの種目の候補に使う種目ライブラリと、過去に記録した種目の ID を端末内へ保存する。
///
/// ログインの後の起動で呼ぶ。通信できないときは保存せず、次の起動で取り直す。
pub async fn refresh_cache() {
    let Ok(list) = all().await else {
        return;
    };
    let Ok(recorded) = recorded_exercises().await else {
        return;
    };
    let Ok(storage) = storage::current().await else {
        return;
    };
    let exercises = list
        .exercises
        .into_iter()
        .map(|item| item.exercise)
        .collect::<Vec<Exercise>>();
    let _ = storage
        .save_alternative_source(&exercises, &recorded.exercise_ids)
        .await;
}

/// 種目のお気に入りを登録または解除する。
pub async fn set_favorite(exercise_id: &str, favorite: bool) -> Result<FavoriteState, HttpError> {
    let method = if favorite { "PUT" } else { "DELETE" };
    http::request_json(method, &format!("/api/exercises/{exercise_id}/favorite")).await
}

/// 使用器具の日本語の名前。
pub fn equipment_label(equipment: Equipment) -> &'static str {
    match equipment {
        Equipment::Barbell => text(Key::LibraryEquipmentBarbell),
        Equipment::Dumbbell => text(Key::LibraryEquipmentDumbbell),
        Equipment::Machine => text(Key::LibraryEquipmentMachine),
        Equipment::Cable => text(Key::LibraryEquipmentCable),
        Equipment::Bodyweight => text(Key::LibraryEquipmentBodyweight),
        Equipment::Other => text(Key::LibraryEquipmentOther),
    }
}

fn encode_query(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_default()
}
