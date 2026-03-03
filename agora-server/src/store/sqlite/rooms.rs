use sqlx::Row;

use crate::store::{
    RoomMemberRecord, RoomRecord, StorageError,
};
use super::SqliteStore;

impl SqliteStore {
    pub async fn create_room_impl(&self, room: &RoomRecord) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO rooms (room_id, name, topic, creator, created_at, room_type) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&room.room_id)
        .bind(&room.name)
        .bind(&room.topic)
        .bind(&room.creator)
        .bind(room.created_at)
        .bind(&room.room_type)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_room_impl(&self, room_id: &str) -> Result<Option<RoomRecord>, StorageError> {
        let row = sqlx::query(
            "SELECT room_id, name, topic, creator, created_at, room_type FROM rooms WHERE room_id = ?",
        )
        .bind(room_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| RoomRecord {
            room_id: r.get("room_id"),
            name: r.get("name"),
            topic: r.get("topic"),
            creator: r.get("creator"),
            created_at: r.get("created_at"),
            room_type: r.get("room_type"),
        }))
    }

    pub async fn delete_room_impl(&self, room_id: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM events WHERE room_id = ?")
            .bind(room_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        sqlx::query("DELETE FROM room_members WHERE room_id = ?")
            .bind(room_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        sqlx::query("DELETE FROM rooms WHERE room_id = ?")
            .bind(room_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn set_membership_impl(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        ts: i64,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO room_members (room_id, user_id, membership, origin_server_ts)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(room_id, user_id) DO UPDATE
             SET membership = excluded.membership, origin_server_ts = excluded.origin_server_ts",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(membership)
        .bind(ts)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_membership_impl(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<String>, StorageError> {
        let row = sqlx::query(
            "SELECT membership FROM room_members WHERE room_id = ? AND user_id = ?",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| r.get("membership")))
    }

    pub async fn get_joined_rooms_impl(&self, user_id: &str) -> Result<Vec<String>, StorageError> {
        let rows = sqlx::query(
            "SELECT room_id FROM room_members WHERE user_id = ? AND membership = 'join'",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows.iter().map(|r| r.get("room_id")).collect())
    }

    pub async fn get_room_members_impl(
        &self,
        room_id: &str,
    ) -> Result<Vec<RoomMemberRecord>, StorageError> {
        let rows = sqlx::query(
            "SELECT room_id, user_id, membership, origin_server_ts
             FROM room_members WHERE room_id = ?",
        )
        .bind(room_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| RoomMemberRecord {
                room_id: r.get("room_id"),
                user_id: r.get("user_id"),
                membership: r.get("membership"),
                origin_server_ts: r.get("origin_server_ts"),
            })
            .collect())
    }

    pub async fn count_room_members_impl(&self, room_id: &str) -> Result<u64, StorageError> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS cnt FROM room_members WHERE room_id = ? AND membership = 'join'",
        )
        .bind(room_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.get::<i64, _>("cnt") as u64)
    }

    pub async fn create_room_alias_impl(&self, alias: &str, room_id: &str) -> Result<(), StorageError> {
        sqlx::query("INSERT INTO room_aliases (alias, room_id) VALUES (?, ?)")
            .bind(alias)
            .bind(room_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_room_alias_impl(&self, alias: &str) -> Result<Option<String>, StorageError> {
        let row = sqlx::query("SELECT room_id FROM room_aliases WHERE alias = ?")
            .bind(alias)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.map(|r| r.get("room_id")))
    }

    pub async fn delete_room_alias_impl(&self, alias: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM room_aliases WHERE alias = ?")
            .bind(alias)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn set_room_visibility_impl(&self, room_id: &str, visibility: &str) -> Result<(), StorageError> {
        sqlx::query("UPDATE rooms SET visibility = ? WHERE room_id = ?")
            .bind(visibility)
            .bind(room_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_public_rooms_impl(&self, limit: u64, search: Option<&str>) -> Result<Vec<RoomRecord>, StorageError> {
        let rows = if let Some(term) = search {
            let pattern = format!("%{term}%");
            sqlx::query(
                "SELECT room_id, name, topic, creator, created_at, room_type
                 FROM rooms WHERE visibility = 'public' AND (name LIKE ? OR topic LIKE ?)
                 LIMIT ?",
            )
            .bind(&pattern)
            .bind(&pattern)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?
        } else {
            sqlx::query(
                "SELECT room_id, name, topic, creator, created_at, room_type
                 FROM rooms WHERE visibility = 'public' LIMIT ?",
            )
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?
        };

        Ok(rows.iter().map(|r| RoomRecord {
            room_id: r.get("room_id"),
            name: r.get("name"),
            topic: r.get("topic"),
            creator: r.get("creator"),
            created_at: r.get("created_at"),
            room_type: r.get("room_type"),
        }).collect())
    }

    pub async fn get_rooms_with_membership_impl(&self, user_id: &str, membership: &str) -> Result<Vec<String>, StorageError> {
        let rows = sqlx::query(
            "SELECT room_id FROM room_members WHERE user_id = ? AND membership = ?",
        )
        .bind(user_id)
        .bind(membership)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(rows.iter().map(|r| r.get("room_id")).collect())
    }
}
