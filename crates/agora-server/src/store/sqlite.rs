use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;

use agora_core::events::RoomEvent;
use agora_core::identifiers::EventId;

use super::{
    AccessTokenRecord, MediaRecord, RoomMemberRecord, RoomRecord, Storage, StorageError,
    UserRecord,
};

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn open(uri: &str) -> Result<Self, StorageError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(uri)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let store = Self { pool };
        store.run_migrations().await?;
        Ok(store)
    }

    async fn run_migrations(&self) -> Result<(), StorageError> {
        let statements = [
            "CREATE TABLE IF NOT EXISTS users (
                user_id TEXT PRIMARY KEY,
                display_name TEXT,
                password_hash TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
            "CREATE TABLE IF NOT EXISTS access_tokens (
                token TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (user_id) REFERENCES users(user_id)
            )",
            "CREATE TABLE IF NOT EXISTS rooms (
                room_id TEXT PRIMARY KEY,
                name TEXT,
                topic TEXT,
                creator TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                room_type TEXT,
                FOREIGN KEY (creator) REFERENCES users(user_id)
            )",
            "CREATE TABLE IF NOT EXISTS room_members (
                room_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                membership TEXT NOT NULL,
                origin_server_ts INTEGER NOT NULL,
                PRIMARY KEY (room_id, user_id),
                FOREIGN KEY (room_id) REFERENCES rooms(room_id),
                FOREIGN KEY (user_id) REFERENCES users(user_id)
            )",
            "CREATE TABLE IF NOT EXISTS events (
                event_id TEXT PRIMARY KEY,
                room_id TEXT NOT NULL,
                sender TEXT NOT NULL,
                event_type TEXT NOT NULL,
                state_key TEXT,
                content TEXT NOT NULL,
                origin_server_ts INTEGER NOT NULL,
                stream_ordering INTEGER NOT NULL,
                FOREIGN KEY (room_id) REFERENCES rooms(room_id),
                FOREIGN KEY (sender) REFERENCES users(user_id)
            )",
            "CREATE INDEX IF NOT EXISTS idx_events_room_ordering
                ON events(room_id, stream_ordering)",
            "CREATE INDEX IF NOT EXISTS idx_events_state
                ON events(room_id, event_type, state_key)
                WHERE state_key IS NOT NULL",
            "CREATE INDEX IF NOT EXISTS idx_room_members_user
                ON room_members(user_id, membership)",
            "CREATE TABLE IF NOT EXISTS media (
                media_id TEXT NOT NULL,
                server_name TEXT NOT NULL,
                uploader TEXT NOT NULL,
                content_type TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                upload_name TEXT,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (server_name, media_id),
                FOREIGN KEY (uploader) REFERENCES users(user_id)
            )",
        ];

        for sql in &statements {
            sqlx::query(sql)
                .execute(&self.pool)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }

        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl Storage for SqliteStore {
    // -- Users ---------------------------------------------------------------

    async fn create_user(&self, user: &UserRecord) -> Result<(), StorageError> {
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

    async fn get_user(&self, user_id: &str) -> Result<Option<UserRecord>, StorageError> {
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

    // -- Access tokens -------------------------------------------------------

    async fn create_token(&self, token: &AccessTokenRecord) -> Result<(), StorageError> {
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

    async fn get_token(&self, token: &str) -> Result<Option<AccessTokenRecord>, StorageError> {
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

    async fn delete_token(&self, token: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM access_tokens WHERE token = ?")
            .bind(token)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    // -- Rooms ---------------------------------------------------------------

    async fn create_room(&self, room: &RoomRecord) -> Result<(), StorageError> {
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

    async fn get_room(&self, room_id: &str) -> Result<Option<RoomRecord>, StorageError> {
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

    async fn delete_room(&self, room_id: &str) -> Result<(), StorageError> {
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

    // -- Room membership -----------------------------------------------------

    async fn set_membership(
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

    async fn get_membership(
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

    async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, StorageError> {
        let rows = sqlx::query(
            "SELECT room_id FROM room_members WHERE user_id = ? AND membership = 'join'",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows.iter().map(|r| r.get("room_id")).collect())
    }

    async fn get_room_members(
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

    async fn count_room_members(&self, room_id: &str) -> Result<u64, StorageError> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS cnt FROM room_members WHERE room_id = ? AND membership = 'join'",
        )
        .bind(room_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.get::<i64, _>("cnt") as u64)
    }

    // -- Events --------------------------------------------------------------

    async fn store_event(&self, event: &RoomEvent) -> Result<i64, StorageError> {
        let content = serde_json::to_string(&event.content)
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let result = sqlx::query(
            "INSERT INTO events (event_id, room_id, sender, event_type, state_key, content, origin_server_ts, stream_ordering)
             VALUES (?, ?, ?, ?, ?, ?, ?,
                     (SELECT COALESCE(MAX(stream_ordering), 0) + 1 FROM events))
             RETURNING stream_ordering",
        )
        .bind(event.event_id.as_str())
        .bind(event.room_id.as_str())
        .bind(event.sender.as_str())
        .bind(&event.event_type)
        .bind(&event.state_key)
        .bind(&content)
        .bind(event.origin_server_ts as i64)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(result.get("stream_ordering"))
    }

    async fn get_events_in_room(
        &self,
        room_id: &str,
        from_ordering: Option<i64>,
        limit: u64,
        direction_forward: bool,
    ) -> Result<Vec<RoomEvent>, StorageError> {
        let (op, order) = if direction_forward {
            (">", "ASC")
        } else {
            ("<", "DESC")
        };

        let from = from_ordering.unwrap_or(if direction_forward { 0 } else { i64::MAX });

        let sql = format!(
            "SELECT event_id, room_id, sender, event_type, state_key, content, origin_server_ts, stream_ordering
             FROM events
             WHERE room_id = ? AND stream_ordering {op} ?
             ORDER BY stream_ordering {order}
             LIMIT ?",
        );

        let rows = sqlx::query(&sql)
            .bind(room_id)
            .bind(from)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows.iter().map(row_to_event).collect())
    }

    async fn get_state_events(
        &self,
        room_id: &str,
    ) -> Result<Vec<RoomEvent>, StorageError> {
        let rows = sqlx::query(
            "SELECT e.event_id, e.room_id, e.sender, e.event_type, e.state_key, e.content, e.origin_server_ts, e.stream_ordering
             FROM events e
             INNER JOIN (
                 SELECT event_type, state_key, MAX(stream_ordering) AS max_ord
                 FROM events
                 WHERE room_id = ? AND state_key IS NOT NULL
                 GROUP BY event_type, state_key
             ) latest ON e.event_type = latest.event_type
                 AND e.state_key = latest.state_key
                 AND e.stream_ordering = latest.max_ord
             WHERE e.room_id = ?",
        )
        .bind(room_id)
        .bind(room_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows.iter().map(row_to_event).collect())
    }

    async fn get_events_since(
        &self,
        room_id: &str,
        since: i64,
    ) -> Result<Vec<RoomEvent>, StorageError> {
        let rows = sqlx::query(
            "SELECT event_id, room_id, sender, event_type, state_key, content, origin_server_ts, stream_ordering
             FROM events
             WHERE room_id = ? AND stream_ordering > ?
             ORDER BY stream_ordering ASC",
        )
        .bind(room_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows.iter().map(row_to_event).collect())
    }

    async fn get_max_stream_ordering(&self) -> Result<i64, StorageError> {
        let row = sqlx::query("SELECT COALESCE(MAX(stream_ordering), 0) AS max_ord FROM events")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.get("max_ord"))
    }

    // -- Media ---------------------------------------------------------------

    async fn store_media(&self, record: &MediaRecord) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO media (media_id, server_name, uploader, content_type, file_size, upload_name, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.media_id)
        .bind(&record.server_name)
        .bind(&record.uploader)
        .bind(&record.content_type)
        .bind(record.file_size)
        .bind(&record.upload_name)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_media(
        &self,
        server_name: &str,
        media_id: &str,
    ) -> Result<Option<MediaRecord>, StorageError> {
        let row = sqlx::query(
            "SELECT media_id, server_name, uploader, content_type, file_size, upload_name, created_at
             FROM media WHERE server_name = ? AND media_id = ?",
        )
        .bind(server_name)
        .bind(media_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| MediaRecord {
            media_id: r.get("media_id"),
            server_name: r.get("server_name"),
            uploader: r.get("uploader"),
            content_type: r.get("content_type"),
            file_size: r.get("file_size"),
            upload_name: r.get("upload_name"),
            created_at: r.get("created_at"),
        }))
    }
}

fn row_to_event(r: &sqlx::sqlite::SqliteRow) -> RoomEvent {
    let content_str: String = r.get("content");
    let content = serde_json::from_str(&content_str).unwrap_or_default();
    let event_id_str: String = r.get("event_id");
    let room_id_str: String = r.get("room_id");
    let sender_str: String = r.get("sender");
    let ordering: i64 = r.get("stream_ordering");

    RoomEvent {
        event_id: EventId::parse(&event_id_str).unwrap_or_else(|_| EventId::new()),
        room_id: agora_core::identifiers::RoomId::parse(&room_id_str)
            .unwrap_or_else(|_| agora_core::identifiers::RoomId::parse("!unknown:localhost").unwrap()),
        sender: agora_core::identifiers::UserId::parse(&sender_str)
            .unwrap_or_else(|_| agora_core::identifiers::UserId::parse("@unknown:localhost").unwrap()),
        event_type: r.get("event_type"),
        state_key: r.get("state_key"),
        content,
        origin_server_ts: r.get::<i64, _>("origin_server_ts") as u64,
        stream_ordering: Some(ordering),
    }
}
