pub mod sqlite;

use agora_core::events::RoomEvent;
use async_trait::async_trait;

// ---------------------------------------------------------------------------
// Domain models stored in the database
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UserRecord {
    pub user_id: String,
    pub display_name: Option<String>,
    pub password_hash: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct AccessTokenRecord {
    pub token: String,
    pub user_id: String,
    pub device_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct RoomRecord {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub creator: String,
    pub created_at: i64,
    pub room_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RoomMemberRecord {
    #[allow(dead_code)]
    pub room_id: String,
    pub user_id: String,
    pub membership: String,
    #[allow(dead_code)]
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone)]
pub struct MediaRecord {
    pub media_id: String,
    pub server_name: String,
    pub uploader: String,
    pub content_type: String,
    pub file_size: i64,
    pub upload_name: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct DeviceKeysRecord {
    pub user_id: String,
    pub device_id: String,
    pub algorithms_json: String,
    pub keys_json: String,
    pub signatures_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct OneTimeKeyRecord {
    pub user_id: String,
    pub device_id: String,
    pub key_id: String,
    pub algorithm: String,
    pub key_data: String,
}

#[derive(Debug, Clone)]
pub struct ToDeviceRecord {
    pub id: i64,
    pub recipient_user: String,
    pub recipient_device: String,
    pub sender: String,
    pub event_type: String,
    pub content_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RoomAliasRecord {
    pub alias: String,
    pub room_id: String,
}

// ---------------------------------------------------------------------------
// Storage trait — implemented for each backend
// ---------------------------------------------------------------------------

#[async_trait]
pub trait Storage: Send + Sync + 'static {
    // -- Users ---------------------------------------------------------------
    async fn create_user(&self, user: &UserRecord) -> Result<(), StorageError>;
    async fn get_user(&self, user_id: &str) -> Result<Option<UserRecord>, StorageError>;

    // -- Access tokens -------------------------------------------------------
    async fn create_token(&self, token: &AccessTokenRecord) -> Result<(), StorageError>;
    async fn get_token(&self, token: &str) -> Result<Option<AccessTokenRecord>, StorageError>;
    async fn delete_token(&self, token: &str) -> Result<(), StorageError>;

    // -- Rooms ---------------------------------------------------------------
    async fn create_room(&self, room: &RoomRecord) -> Result<(), StorageError>;
    async fn get_room(&self, room_id: &str) -> Result<Option<RoomRecord>, StorageError>;

    async fn delete_room(&self, room_id: &str) -> Result<(), StorageError>;

    // -- Room membership -----------------------------------------------------
    async fn set_membership(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        ts: i64,
    ) -> Result<(), StorageError>;

    async fn get_membership(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<String>, StorageError>;

    async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, StorageError>;

    async fn get_room_members(&self, room_id: &str) -> Result<Vec<RoomMemberRecord>, StorageError>;

    async fn count_room_members(&self, room_id: &str) -> Result<u64, StorageError>;

    // -- Events --------------------------------------------------------------
    async fn store_event(&self, event: &RoomEvent) -> Result<i64, StorageError>;

    async fn get_events_in_room(
        &self,
        room_id: &str,
        from_ordering: Option<i64>,
        limit: u64,
        direction_forward: bool,
    ) -> Result<Vec<RoomEvent>, StorageError>;

    async fn get_state_events(
        &self,
        room_id: &str,
    ) -> Result<Vec<RoomEvent>, StorageError>;

    async fn get_events_since(
        &self,
        room_id: &str,
        since: i64,
    ) -> Result<Vec<RoomEvent>, StorageError>;

    async fn get_max_stream_ordering(&self) -> Result<i64, StorageError>;

    // -- Media ---------------------------------------------------------------
    async fn store_media(&self, record: &MediaRecord) -> Result<(), StorageError>;

    async fn get_media(
        &self,
        server_name: &str,
        media_id: &str,
    ) -> Result<Option<MediaRecord>, StorageError>;

    // -- E2EE: Device keys ---------------------------------------------------
    async fn upsert_device_keys(&self, record: &DeviceKeysRecord) -> Result<(), StorageError>;

    #[allow(unused)]
    async fn get_device_keys(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DeviceKeysRecord>, StorageError>;

    async fn get_device_keys_for_users(
        &self,
        user_device_pairs: &[(String, Vec<String>)],
    ) -> Result<Vec<DeviceKeysRecord>, StorageError>;

    // -- E2EE: One-time keys -------------------------------------------------
    async fn store_one_time_keys(&self, keys: &[OneTimeKeyRecord]) -> Result<(), StorageError>;

    async fn claim_one_time_key(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<OneTimeKeyRecord>, StorageError>;

    async fn count_one_time_keys(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<std::collections::BTreeMap<String, u64>, StorageError>;

    // -- E2EE: To-device messages --------------------------------------------
    async fn queue_to_device(&self, records: &[ToDeviceRecord]) -> Result<(), StorageError>;

    async fn get_to_device_messages(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<ToDeviceRecord>, StorageError>;

    async fn delete_to_device_messages(&self, ids: &[i64]) -> Result<(), StorageError>;

    // -- Transaction idempotency ---------------------------------------------
    async fn get_txn_event_id(
        &self,
        user_id: &str,
        txn_id: &str,
    ) -> Result<Option<String>, StorageError>;

    async fn store_txn(
        &self,
        user_id: &str,
        txn_id: &str,
        event_id: &str,
    ) -> Result<(), StorageError>;

    // -- Redaction ------------------------------------------------------------
    async fn redact_event(&self, event_id: &str) -> Result<(), StorageError>;

    // -- User profile ---------------------------------------------------------
    async fn update_display_name(
        &self,
        user_id: &str,
        display_name: &str,
    ) -> Result<(), StorageError>;

    async fn update_avatar_url(
        &self,
        user_id: &str,
        avatar_url: &str,
    ) -> Result<(), StorageError>;

    async fn get_avatar_url(&self, user_id: &str) -> Result<Option<String>, StorageError>;

    // -- Room aliases ---------------------------------------------------------
    async fn create_room_alias(&self, alias: &str, room_id: &str) -> Result<(), StorageError>;
    async fn get_room_alias(&self, alias: &str) -> Result<Option<String>, StorageError>;
    async fn delete_room_alias(&self, alias: &str) -> Result<(), StorageError>;

    // -- Room visibility / directory ------------------------------------------
    #[allow(unused)]
    async fn set_room_visibility(
        &self,
        room_id: &str,
        visibility: &str,
    ) -> Result<(), StorageError>;

    async fn get_public_rooms(
        &self,
        limit: u64,
        search: Option<&str>,
    ) -> Result<Vec<RoomRecord>, StorageError>;

    // -- Devices --------------------------------------------------------------
    async fn get_devices_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<AccessTokenRecord>, StorageError>;

    async fn delete_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), StorageError>;

    // -- Membership queries ---------------------------------------------------
    async fn get_rooms_with_membership(
        &self,
        user_id: &str,
        membership: &str,
    ) -> Result<Vec<String>, StorageError>;

    // -- Sequence timestamp recovery -----------------------------------------

    /// Return the maximum `created_at` / `origin_server_ts` value stored in any
    /// table. Used on startup to resume `SequenceTimestamp` past the last persisted
    /// value, preventing token collisions caused by counter reset after a restart.
    async fn get_max_timestamp(&self) -> Result<u64, StorageError>;
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(String),
    #[error("not found")]
    NotFound,
}
