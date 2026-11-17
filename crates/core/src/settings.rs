//! Key-value settings stored in the database (`settings` table, JSON values).

use serde_json::Value;
use sqlx::SqlitePool;

use crate::Ctx;

pub async fn get(db: &SqlitePool, key: &str) -> sqlx::Result<Option<Value>> {
    let row: Option<sqlx::types::Json<Value>> =
        sqlx::query_scalar(r#"SELECT value FROM settings WHERE "key" = ?"#).bind(key).fetch_optional(db).await?;
    Ok(row.map(|j| j.0))
}

/// `None` removes the setting.
pub async fn set(db: &SqlitePool, key: &str, value: Option<Value>) -> sqlx::Result<()> {
    match value {
        None => {
            sqlx::query(r#"DELETE FROM settings WHERE "key" = ?"#).bind(key).execute(db).await?;
        }
        Some(v) => {
            sqlx::query(
                r#"INSERT INTO settings ("key", value) VALUES (?, ?)
                   ON CONFLICT("key") DO UPDATE SET value = excluded.value"#,
            )
            .bind(key)
            .bind(sqlx::types::Json(v))
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

/// The saved Hugging Face token (not the environment one).
pub async fn saved_hf_token(db: &SqlitePool) -> Option<String> {
    match get(db, "hf_token").await {
        Ok(Some(Value::String(s))) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// The Hugging Face token to use: HF_TOKEN from the environment, else the saved one.
pub async fn hf_token(ctx: &Ctx) -> Option<String> {
    if let Some(t) = &ctx.config.env_hf_token {
        return Some(t.clone());
    }
    saved_hf_token(&ctx.db).await
}

/// "…a9f2" for secrets longer than 8 characters, "…" for shorter ones, None when unset.
pub fn key_hint(secret: Option<&str>) -> Option<String> {
    let s = secret.filter(|s| !s.is_empty())?;
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > 8 {
        Some(format!("…{}", chars[chars.len() - 4..].iter().collect::<String>()))
    } else {
        Some("…".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn set_get_remove() {
        let db = crate::db::connect_memory().await.unwrap();
        assert!(get(&db, "hf_token").await.unwrap().is_none());
        set(&db, "hf_token", Some(Value::String("hf_abcdefghij".into()))).await.unwrap();
        assert_eq!(saved_hf_token(&db).await.as_deref(), Some("hf_abcdefghij"));
        set(&db, "hf_token", Some(Value::String("hf_x".into()))).await.unwrap();
        assert_eq!(saved_hf_token(&db).await.as_deref(), Some("hf_x"));
        set(&db, "hf_token", None).await.unwrap();
        assert!(saved_hf_token(&db).await.is_none());
    }

    #[test]
    fn hints() {
        assert_eq!(key_hint(Some("sk-or-v1-0000000000000000a9f2")).as_deref(), Some("…a9f2"));
        assert_eq!(key_hint(Some("short")).as_deref(), Some("…"));
        assert_eq!(key_hint(Some("")), None);
    }
}
