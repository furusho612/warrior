//! プレースホルダ付きの SQL と、利用者に属するクエリの組み立て。
//!
//! 利用者に属するテーブルへのクエリは `user_id` の条件を必ず持つ (ADR-015)。
//! その条件を型で表し、条件の無いクエリを組み立てられないようにする。

/// 利用者に属するテーブルの利用者 ID の列名。
pub const USER_ID_COLUMN: &str = "user_id";

/// SQL に渡すバインド値。
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    /// 文字列。
    Text(String),
    /// 整数。
    Integer(i64),
    /// 小数。
    Real(f64),
    /// NULL。
    Null,
}

/// プレースホルダ付きの SQL と、そのバインド値。
#[derive(Debug, Clone, PartialEq)]
pub struct SqlStatement {
    sql: String,
    bindings: Vec<SqlValue>,
}

impl SqlStatement {
    /// SQL とバインド値の組を作る。
    pub fn new(sql: impl Into<String>, bindings: Vec<SqlValue>) -> Self {
        Self {
            sql: sql.into(),
            bindings,
        }
    }

    /// プレースホルダ付きの SQL。
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// SQL のプレースホルダに順に対応するバインド値。
    pub fn bindings(&self) -> &[SqlValue] {
        &self.bindings
    }
}

/// 利用者に属するクエリの組み立ての誤り。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlAssemblyError {
    /// 利用者 ID の条件が無い。
    MissingUserIdCondition,
    /// 利用者 ID の条件に対応するバインド値が先頭に無い。
    MissingUserIdBinding,
    /// 識別子として使えない文字列がある。
    InvalidIdentifier(String),
    /// 取得する列が空である。
    EmptyColumnList,
}

/// 利用者に属するテーブルへのクエリ。
///
/// この型は `user_id` の条件を必ず持ち、その条件のバインド値を先頭に持つ。
#[derive(Debug, Clone, PartialEq)]
pub struct UserScopedStatement {
    statement: SqlStatement,
}

impl UserScopedStatement {
    /// 利用者 ID の条件を持つクエリを検証して作る。
    pub fn new(user_id: &str, statement: SqlStatement) -> Result<Self, SqlAssemblyError> {
        if !statement.sql.contains("user_id = ?") {
            return Err(SqlAssemblyError::MissingUserIdCondition);
        }
        match statement.bindings.first() {
            Some(SqlValue::Text(bound_user_id)) if bound_user_id == user_id => {}
            _ => return Err(SqlAssemblyError::MissingUserIdBinding),
        }
        Ok(Self { statement })
    }

    /// 利用者の行から指定した列をすべて取得するクエリを組み立てる。
    pub fn select_all(
        user_id: &str,
        table: &str,
        columns: &[&str],
    ) -> Result<Self, SqlAssemblyError> {
        validate_identifier(table)?;
        if columns.is_empty() {
            return Err(SqlAssemblyError::EmptyColumnList);
        }
        for column in columns {
            validate_identifier(column)?;
        }

        let sql = format!(
            "SELECT {} FROM {} WHERE user_id = ?",
            columns.join(", "),
            table
        );
        Self::new(
            user_id,
            SqlStatement::new(sql, vec![SqlValue::Text(user_id.to_owned())]),
        )
    }

    /// 組み立てたクエリ。
    pub fn statement(&self) -> &SqlStatement {
        &self.statement
    }
}

/// SQL の識別子に使える文字列かを検証する。
///
/// 英小文字、数字、`_` だけを許可し、先頭を英小文字にする。
fn validate_identifier(identifier: &str) -> Result<(), SqlAssemblyError> {
    let mut characters = identifier.chars();
    let Some(first) = characters.next() else {
        return Err(SqlAssemblyError::InvalidIdentifier(identifier.to_owned()));
    };
    if !first.is_ascii_lowercase() {
        return Err(SqlAssemblyError::InvalidIdentifier(identifier.to_owned()));
    }
    if !characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    }) {
        return Err(SqlAssemblyError::InvalidIdentifier(identifier.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SqlAssemblyError, SqlStatement, SqlValue, UserScopedStatement, USER_ID_COLUMN};

    const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

    #[test]
    fn user_id_column_is_named_user_id() {
        assert_eq!(USER_ID_COLUMN, "user_id");
    }

    #[test]
    fn select_all_includes_user_id_condition_and_binding() {
        let query =
            UserScopedStatement::select_all(USER_ID, "trainings", &["id", "started_at"]).unwrap();

        assert_eq!(
            query.statement().sql(),
            "SELECT id, started_at FROM trainings WHERE user_id = ?"
        );
        assert_eq!(
            query.statement().bindings(),
            &[SqlValue::Text(USER_ID.to_owned())]
        );
    }

    #[test]
    fn rejects_statement_without_user_id_condition() {
        let statement = SqlStatement::new(
            "SELECT id FROM trainings",
            vec![SqlValue::Text(USER_ID.to_owned())],
        );

        assert_eq!(
            UserScopedStatement::new(USER_ID, statement),
            Err(SqlAssemblyError::MissingUserIdCondition)
        );
    }

    #[test]
    fn rejects_statement_with_mismatched_user_id_binding() {
        let statement = SqlStatement::new(
            "SELECT id FROM trainings WHERE user_id = ?",
            vec![SqlValue::Text(
                "00000000-0000-4000-8000-000000000000".to_owned(),
            )],
        );

        assert_eq!(
            UserScopedStatement::new(USER_ID, statement),
            Err(SqlAssemblyError::MissingUserIdBinding)
        );
    }

    #[test]
    fn rejects_statement_without_binding() {
        let statement = SqlStatement::new("SELECT id FROM trainings WHERE user_id = ?", Vec::new());

        assert_eq!(
            UserScopedStatement::new(USER_ID, statement),
            Err(SqlAssemblyError::MissingUserIdBinding)
        );
    }

    #[test]
    fn rejects_invalid_table_identifier() {
        assert_eq!(
            UserScopedStatement::select_all(USER_ID, "trainings; DROP TABLE users", &["id"]),
            Err(SqlAssemblyError::InvalidIdentifier(
                "trainings; DROP TABLE users".to_owned()
            ))
        );
    }

    #[test]
    fn rejects_invalid_column_identifier() {
        assert_eq!(
            UserScopedStatement::select_all(USER_ID, "trainings", &["id", "1"]),
            Err(SqlAssemblyError::InvalidIdentifier("1".to_owned()))
        );
    }

    #[test]
    fn rejects_empty_column_list() {
        assert_eq!(
            UserScopedStatement::select_all(USER_ID, "trainings", &[]),
            Err(SqlAssemblyError::EmptyColumnList)
        );
    }
}
