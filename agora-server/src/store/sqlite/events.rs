use agora_core::events::RoomEvent;
use sqlx::Row;

use crate::store::StorageError;
use super::{SqliteStore, row_to_event};

impl SqliteStore {
    pub async fn store_event_impl(&self, event: &RoomEvent) -> Result<i64, StorageError> {
        let content = serde_json::to_string(&event.content)
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let result = sqlx::query(
            "INSERT INTO events (event_id, room_id, sender, event_type, state_key, content, origin_server_ts)
             VALUES (?, ?, ?, ?, ?, ?, ?)
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

    pub async fn get_events_in_room_impl(
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

    pub async fn get_state_events_impl(
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

    pub async fn get_events_since_impl(
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

    pub async fn get_max_stream_ordering_impl(&self) -> Result<i64, StorageError> {
        let row = sqlx::query("SELECT COALESCE(MAX(stream_ordering), 0) AS max_ord FROM events")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.get("max_ord"))
    }

    pub async fn redact_event_impl(&self, event_id: &str) -> Result<(), StorageError> {
        let row = sqlx::query("SELECT event_type, content FROM events WHERE event_id = ?")
            .bind(event_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        if let Some(r) = row {
            let event_type: String = r.get("event_type");
            let content_str: String = r.get("content");
            let content: serde_json::Value = serde_json::from_str(&content_str).unwrap_or_default();

            let redacted = match event_type.as_str() {
                "m.room.member" => {
                    let mut obj = serde_json::Map::new();
                    if let Some(m) = content.get("membership") {
                        obj.insert("membership".to_owned(), m.clone());
                    }
                    serde_json::Value::Object(obj)
                }
                "m.room.create" => content,
                "m.room.join_rules" => {
                    let mut obj = serde_json::Map::new();
                    if let Some(j) = content.get("join_rule") {
                        obj.insert("join_rule".to_owned(), j.clone());
                    }
                    serde_json::Value::Object(obj)
                }
                "m.room.power_levels" => content,
                _ => serde_json::json!({}),
            };

            let redacted_str = serde_json::to_string(&redacted).unwrap_or_else(|_| "{}".to_owned());
            sqlx::query("UPDATE events SET content = ? WHERE event_id = ?")
                .bind(&redacted_str)
                .bind(event_id)
                .execute(&self.pool)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn get_txn_event_id_impl(&self, user_id: &str, txn_id: &str) -> Result<Option<String>, StorageError> {
        let row = sqlx::query("SELECT event_id FROM sent_transactions WHERE user_id = ? AND txn_id = ?")
            .bind(user_id)
            .bind(txn_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.map(|r| r.get("event_id")))
    }

    pub async fn store_txn_impl(&self, user_id: &str, txn_id: &str, event_id: &str) -> Result<(), StorageError> {
        sqlx::query("INSERT OR IGNORE INTO sent_transactions (user_id, txn_id, event_id) VALUES (?, ?, ?)")
            .bind(user_id)
            .bind(txn_id)
            .bind(event_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }
}
