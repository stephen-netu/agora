use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;

use agora_core::events::RoomEvent;
use agora_core::identifiers::EventId;

use super::{
    AccessTokenRecord, DeviceKeysRecord, MediaRecord, OneTimeKeyRecord,
    RoomMemberRecord, RoomRecord, Storage, StorageError, ToDeviceRecord, UserRecord,
};

mod users;
mod rooms;
mod events;
mod e2ee;
mod media;

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
            "CREATE TABLE IF NOT EXISTS device_keys (
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                algorithms_json TEXT NOT NULL,
                keys_json TEXT NOT NULL,
                signatures_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (user_id, device_id)
            )",
            "CREATE TABLE IF NOT EXISTS one_time_keys (
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                key_id TEXT NOT NULL,
                algorithm TEXT NOT NULL,
                key_data TEXT NOT NULL,
                PRIMARY KEY (user_id, device_id, key_id)
            )",
            "CREATE TABLE IF NOT EXISTS to_device_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recipient_user TEXT NOT NULL,
                recipient_device TEXT NOT NULL,
                sender TEXT NOT NULL,
                event_type TEXT NOT NULL,
                content_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
            "CREATE INDEX IF NOT EXISTS idx_to_device_recipient
                ON to_device_messages(recipient_user, recipient_device)",
            "CREATE TABLE IF NOT EXISTS sent_transactions (
                user_id TEXT NOT NULL,
                txn_id TEXT NOT NULL,
                event_id TEXT NOT NULL,
                PRIMARY KEY (user_id, txn_id)
            )",
            "CREATE TABLE IF NOT EXISTS room_aliases (
                alias TEXT PRIMARY KEY,
                room_id TEXT NOT NULL
            )",
        ];

        let alter_statements = [
            "ALTER TABLE users ADD COLUMN avatar_url TEXT",
            "ALTER TABLE rooms ADD COLUMN visibility TEXT DEFAULT 'private'",
        ];

        for sql in &statements {
            sqlx::query(sql)
                .execute(&self.pool)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }

        for sql in &alter_statements {
            let _ = sqlx::query(sql).execute(&self.pool).await;
        }

        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
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

#[async_trait]
impl Storage for SqliteStore {
    async fn create_user(&self, user: &UserRecord) -> Result<(), StorageError> {
        self.create_user_impl(user).await
    }

    async fn get_user(&self, user_id: &str) -> Result<Option<UserRecord>, StorageError> {
        self.get_user_impl(user_id).await
    }

    async fn create_token(&self, token: &AccessTokenRecord) -> Result<(), StorageError> {
        self.create_token_impl(token).await
    }

    async fn get_token(&self, token: &str) -> Result<Option<AccessTokenRecord>, StorageError> {
        self.get_token_impl(token).await
    }

    async fn delete_token(&self, token: &str) -> Result<(), StorageError> {
        self.delete_token_impl(token).await
    }

    async fn create_room(&self, room: &RoomRecord) -> Result<(), StorageError> {
        self.create_room_impl(room).await
    }

    async fn get_room(&self, room_id: &str) -> Result<Option<RoomRecord>, StorageError> {
        self.get_room_impl(room_id).await
    }

    async fn delete_room(&self, room_id: &str) -> Result<(), StorageError> {
        self.delete_room_impl(room_id).await
    }

    async fn set_membership(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        ts: i64,
    ) -> Result<(), StorageError> {
        self.set_membership_impl(room_id, user_id, membership, ts).await
    }

    async fn get_membership(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<String>, StorageError> {
        self.get_membership_impl(room_id, user_id).await
    }

    async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, StorageError> {
        self.get_joined_rooms_impl(user_id).await
    }

    async fn get_room_members(
        &self,
        room_id: &str,
    ) -> Result<Vec<RoomMemberRecord>, StorageError> {
        self.get_room_members_impl(room_id).await
    }

    async fn count_room_members(&self, room_id: &str) -> Result<u64, StorageError> {
        self.count_room_members_impl(room_id).await
    }

    async fn store_event(&self, event: &RoomEvent) -> Result<i64, StorageError> {
        self.store_event_impl(event).await
    }

    async fn get_events_in_room(
        &self,
        room_id: &str,
        from_ordering: Option<i64>,
        limit: u64,
        direction_forward: bool,
    ) -> Result<Vec<RoomEvent>, StorageError> {
        self.get_events_in_room_impl(room_id, from_ordering, limit, direction_forward).await
    }

    async fn get_state_events(
        &self,
        room_id: &str,
    ) -> Result<Vec<RoomEvent>, StorageError> {
        self.get_state_events_impl(room_id).await
    }

    async fn get_events_since(
        &self,
        room_id: &str,
        since: i64,
    ) -> Result<Vec<RoomEvent>, StorageError> {
        self.get_events_since_impl(room_id, since).await
    }

    async fn get_max_stream_ordering(&self) -> Result<i64, StorageError> {
        self.get_max_stream_ordering_impl().await
    }

    async fn store_media(&self, record: &MediaRecord) -> Result<(), StorageError> {
        self.store_media_impl(record).await
    }

    async fn get_media(
        &self,
        server_name: &str,
        media_id: &str,
    ) -> Result<Option<MediaRecord>, StorageError> {
        self.get_media_impl(server_name, media_id).await
    }

    async fn upsert_device_keys(&self, record: &DeviceKeysRecord) -> Result<(), StorageError> {
        self.upsert_device_keys_impl(record).await
    }

    async fn get_device_keys(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DeviceKeysRecord>, StorageError> {
        self.get_device_keys_impl(user_id, device_id).await
    }

    async fn get_device_keys_for_users(
        &self,
        user_device_pairs: &[(String, Vec<String>)],
    ) -> Result<Vec<DeviceKeysRecord>, StorageError> {
        self.get_device_keys_for_users_impl(user_device_pairs).await
    }

    async fn store_one_time_keys(&self, keys: &[OneTimeKeyRecord]) -> Result<(), StorageError> {
        self.store_one_time_keys_impl(keys).await
    }

    async fn claim_one_time_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<OneTimeKeyRecord>, StorageError> {
        self.claim_one_time_key_impl(user_id, device_id, algorithm).await
    }

    async fn count_one_time_keys(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<std::collections::HashMap<String, u64>, StorageError> {
        self.count_one_time_keys_impl(user_id, device_id).await
    }

    async fn queue_to_device(&self, records: &[ToDeviceRecord]) -> Result<(), StorageError> {
        self.queue_to_device_impl(records).await
    }

    async fn get_to_device_messages(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<ToDeviceRecord>, StorageError> {
        self.get_to_device_messages_impl(user_id, device_id).await
    }

    async fn delete_to_device_messages(&self, ids: &[i64]) -> Result<(), StorageError> {
        self.delete_to_device_messages_impl(ids).await
    }

    async fn get_txn_event_id(&self, user_id: &str, txn_id: &str) -> Result<Option<String>, StorageError> {
        self.get_txn_event_id_impl(user_id, txn_id).await
    }

    async fn store_txn(&self, user_id: &str, txn_id: &str, event_id: &str) -> Result<(), StorageError> {
        self.store_txn_impl(user_id, txn_id, event_id).await
    }

    async fn redact_event(&self, event_id: &str) -> Result<(), StorageError> {
        self.redact_event_impl(event_id).await
    }

    async fn update_display_name(&self, user_id: &str, display_name: &str) -> Result<(), StorageError> {
        self.update_display_name_impl(user_id, display_name).await
    }

    async fn update_avatar_url(&self, user_id: &str, avatar_url: &str) -> Result<(), StorageError> {
        self.update_avatar_url_impl(user_id, avatar_url).await
    }

    async fn get_avatar_url(&self, user_id: &str) -> Result<Option<String>, StorageError> {
        self.get_avatar_url_impl(user_id).await
    }

    async fn create_room_alias(&self, alias: &str, room_id: &str) -> Result<(), StorageError> {
        self.create_room_alias_impl(alias, room_id).await
    }

    async fn get_room_alias(&self, alias: &str) -> Result<Option<String>, StorageError> {
        self.get_room_alias_impl(alias).await
    }

    async fn delete_room_alias(&self, alias: &str) -> Result<(), StorageError> {
        self.delete_room_alias_impl(alias).await
    }

    async fn set_room_visibility(&self, room_id: &str, visibility: &str) -> Result<(), StorageError> {
        self.set_room_visibility_impl(room_id, visibility).await
    }

    async fn get_public_rooms(&self, limit: u64, search: Option<&str>) -> Result<Vec<RoomRecord>, StorageError> {
        self.get_public_rooms_impl(limit, search).await
    }

    async fn get_devices_for_user(&self, user_id: &str) -> Result<Vec<AccessTokenRecord>, StorageError> {
        self.get_devices_for_user_impl(user_id).await
    }

    async fn delete_device(&self, user_id: &str, device_id: &str) -> Result<(), StorageError> {
        self.delete_device_impl(user_id, device_id).await
    }

    async fn get_rooms_with_membership(&self, user_id: &str, membership: &str) -> Result<Vec<String>, StorageError> {
        self.get_rooms_with_membership_impl(user_id, membership).await
    }
}
