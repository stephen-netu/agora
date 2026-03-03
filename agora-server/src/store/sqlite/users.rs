use sqlx::Row;

use crate::store::{
    AccessTokenRecord, StorageError, UserRecord,
};
use super::SqliteStore;

impl SqliteStore {
    pub async fn create_user_impl(&self, user: &UserRecord) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO users (user_id, display_name, password_hash, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&user.user_id)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(user.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_user_impl(&self, user_id: &str) -> Result<Option<UserRecord>, StorageError> {
        let row = sqlx::query(
            "SELECT user_id, display_name, password_hash, created_at FROM users WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| UserRecord {
            user_id: r.get("user_id"),
            display_name: r.get("display_name"),
            password_hash: r.get("password_hash"),
            created_at: r.get("created_at"),
        }))
    }

    pub async fn create_token_impl(&self, token: &AccessTokenRecord) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO access_tokens (token, user_id, device_id, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&token.token)
        .bind(&token.user_id)
        .bind(&token.device_id)
        .bind(token.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_token_impl(&self, token: &str) -> Result<Option<AccessTokenRecord>, StorageError> {
        let row = sqlx::query(
            "SELECT token, user_id, device_id, created_at FROM access_tokens WHERE token = ?",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| AccessTokenRecord {
            token: r.get("token"),
            user_id: r.get("user_id"),
            device_id: r.get("device_id"),
            created_at: r.get("created_at"),
        }))
    }

    pub async fn delete_token_impl(&self, token: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM access_tokens WHERE token = ?")
            .bind(token)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn update_display_name_impl(&self, user_id: &str, display_name: &str) -> Result<(), StorageError> {
        sqlx::query("UPDATE users SET display_name = ? WHERE user_id = ?")
            .bind(display_name)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn update_avatar_url_impl(&self, user_id: &str, avatar_url: &str) -> Result<(), StorageError> {
        sqlx::query("UPDATE users SET avatar_url = ? WHERE user_id = ?")
            .bind(avatar_url)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_avatar_url_impl(&self, user_id: &str) -> Result<Option<String>, StorageError> {
        let row = sqlx::query("SELECT avatar_url FROM users WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.and_then(|r| r.get("avatar_url")))
    }

    pub async fn get_devices_for_user_impl(&self, user_id: &str) -> Result<Vec<AccessTokenRecord>, StorageError> {
        let rows = sqlx::query(
            "SELECT token, user_id, device_id, created_at FROM access_tokens WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows.iter().map(|r| AccessTokenRecord {
            token: r.get("token"),
            user_id: r.get("user_id"),
            device_id: r.get("device_id"),
            created_at: r.get("created_at"),
        }).collect())
    }

    pub async fn delete_device_impl(&self, user_id: &str, device_id: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM access_tokens WHERE user_id = ? AND device_id = ?")
            .bind(user_id)
            .bind(device_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }
}
