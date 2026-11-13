//! D1 の prepare と batch の薄いラッパー。
//!
//! SQL の組み立ては `warrior_core` が持ち、このモジュールはバインド値の変換と実行だけを行う。
//! 複数の文をまとめて書き込むときは `batch` を使う (ADR-015)。

use warrior_core::sql::{SqlStatement, SqlValue};
use wasm_bindgen::JsValue;
use worker::d1::{D1Database, D1PreparedStatement, D1Result};
use worker::Result;

/// D1 のバインディングを包むデータベース。
pub struct Db {
    database: D1Database,
}

impl Db {
    /// D1 のバインディングから作る。
    pub fn new(database: D1Database) -> Self {
        Self { database }
    }

    /// 1 つの文を実行する。
    pub async fn run(&self, statement: &SqlStatement) -> Result<D1Result> {
        self.prepare(statement)?.run().await
    }

    /// 1 つの文を実行し、すべての行を返す。
    pub async fn all(&self, statement: &SqlStatement) -> Result<D1Result> {
        self.prepare(statement)?.all().await
    }

    /// 複数の文を 1 つのトランザクションとして実行する。
    pub async fn batch(&self, statements: &[SqlStatement]) -> Result<Vec<D1Result>> {
        let prepared = statements
            .iter()
            .map(|statement| self.prepare(statement))
            .collect::<Result<Vec<D1PreparedStatement>>>()?;
        self.database.batch(prepared).await
    }

    fn prepare(&self, statement: &SqlStatement) -> Result<D1PreparedStatement> {
        let bindings = statement
            .bindings()
            .iter()
            .map(sql_value_to_js)
            .collect::<Vec<JsValue>>();
        self.database.prepare(statement.sql()).bind(&bindings)
    }
}

/// SQL のバインド値を D1 が受け取る JavaScript の値にする。
///
/// D1 の整数は JavaScript の数値で渡すため、2^53 を超える値は精度を失う。
/// アプリが整数で渡すのは回数と件数に限る。
fn sql_value_to_js(value: &SqlValue) -> JsValue {
    match value {
        SqlValue::Text(text) => JsValue::from_str(text),
        SqlValue::Integer(value) => JsValue::from_f64(*value as f64),
        SqlValue::Real(value) => JsValue::from_f64(*value),
        SqlValue::Null => JsValue::NULL,
    }
}
