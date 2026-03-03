use sqlx::Row;

use crate::store::{
    MediaRecord, StorageError,
};
use super::SqliteStore;

impl SqliteStore {
    pub async fn store_media_impl(&self, record: &MediaRecord) -> Result<(), StorageError> {
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

    pub async fn get_media_impl(
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
