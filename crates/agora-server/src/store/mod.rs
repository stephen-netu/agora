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
}

#[derive(Debug, Clone)]
pub struct RoomMemberRecord {
    pub room_id: String,
    pub user_id: String,
    pub membership: String,
    pub origin_server_ts: i64,
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
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(String),
    #[error("not found")]
    NotFound,
}
