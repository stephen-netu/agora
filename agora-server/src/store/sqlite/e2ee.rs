use sqlx::Row;

use crate::store::{
    DeviceKeysRecord, OneTimeKeyRecord, StorageError, ToDeviceRecord,
};
use super::SqliteStore;

impl SqliteStore {
    pub async fn upsert_device_keys_impl(&self, record: &DeviceKeysRecord) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO device_keys (user_id, device_id, algorithms_json, keys_json, signatures_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, device_id) DO UPDATE
             SET algorithms_json = excluded.algorithms_json,
                 keys_json = excluded.keys_json,
                 signatures_json = excluded.signatures_json",
        )
        .bind(&record.user_id)
        .bind(&record.device_id)
        .bind(&record.algorithms_json)
        .bind(&record.keys_json)
        .bind(&record.signatures_json)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get_device_keys_impl(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DeviceKeysRecord>, StorageError> {
        let row = sqlx::query(
            "SELECT user_id, device_id, algorithms_json, keys_json, signatures_json, created_at
             FROM device_keys WHERE user_id = ? AND device_id = ?",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| DeviceKeysRecord {
            user_id: r.get("user_id"),
            device_id: r.get("device_id"),
            algorithms_json: r.get("algorithms_json"),
            keys_json: r.get("keys_json"),
            signatures_json: r.get("signatures_json"),
            created_at: r.get("created_at"),
        }))
    }

    pub async fn get_device_keys_for_users_impl(
        &self,
        user_device_pairs: &[(String, Vec<String>)],
    ) -> Result<Vec<DeviceKeysRecord>, StorageError> {
        let mut results = Vec::new();
        for (user_id, device_ids) in user_device_pairs {
            let rows = if device_ids.is_empty() {
                sqlx::query(
                    "SELECT user_id, device_id, algorithms_json, keys_json, signatures_json, created_at
                     FROM device_keys WHERE user_id = ?",
                )
                .bind(user_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?
            } else {
                let placeholders: Vec<&str> = device_ids.iter().map(|_| "?").collect();
                let sql = format!(
                    "SELECT user_id, device_id, algorithms_json, keys_json, signatures_json, created_at
                     FROM device_keys WHERE user_id = ? AND device_id IN ({})",
                    placeholders.join(",")
                );
                let mut q = sqlx::query(&sql).bind(user_id);
                for did in device_ids {
                    q = q.bind(did);
                }
                q.fetch_all(&self.pool)
                    .await
                    .map_err(|e| StorageError::Database(e.to_string()))?
            };
            for r in &rows {
                results.push(DeviceKeysRecord {
                    user_id: r.get("user_id"),
                    device_id: r.get("device_id"),
                    algorithms_json: r.get("algorithms_json"),
                    keys_json: r.get("keys_json"),
                    signatures_json: r.get("signatures_json"),
                    created_at: r.get("created_at"),
                });
            }
        }
        Ok(results)
    }

    pub async fn store_one_time_keys_impl(&self, keys: &[OneTimeKeyRecord]) -> Result<(), StorageError> {
        for k in keys {
            sqlx::query(
                "INSERT OR IGNORE INTO one_time_keys (user_id, device_id, key_id, algorithm, key_data)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&k.user_id)
            .bind(&k.device_id)
            .bind(&k.key_id)
            .bind(&k.algorithm)
            .bind(&k.key_data)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn claim_one_time_key_impl(
        &self,
        user_id: &str,
        device_id: &str,
        algorithm: &str,
    ) -> Result<Option<OneTimeKeyRecord>, StorageError> {
        let row = sqlx::query(
            "SELECT user_id, device_id, key_id, algorithm, key_data
             FROM one_time_keys
             WHERE user_id = ? AND device_id = ? AND algorithm = ?
             LIMIT 1",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(algorithm)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        if let Some(r) = row {
            let record = OneTimeKeyRecord {
                user_id: r.get("user_id"),
                device_id: r.get("device_id"),
                key_id: r.get("key_id"),
                algorithm: r.get("algorithm"),
                key_data: r.get("key_data"),
            };
            sqlx::query(
                "DELETE FROM one_time_keys WHERE user_id = ? AND device_id = ? AND key_id = ?",
            )
            .bind(&record.user_id)
            .bind(&record.device_id)
            .bind(&record.key_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    pub async fn count_one_time_keys_impl(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<std::collections::BTreeMap<String, u64>, StorageError> {
        let rows = sqlx::query(
            "SELECT algorithm, COUNT(*) AS cnt
             FROM one_time_keys
             WHERE user_id = ? AND device_id = ?
             GROUP BY algorithm",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        let mut counts = std::collections::BTreeMap::new();
        for r in &rows {
            let alg: String = r.get("algorithm");
            let cnt: i64 = r.get("cnt");
            counts.insert(alg, cnt as u64);
        }
        Ok(counts)
    }

    pub async fn queue_to_device_impl(&self, records: &[ToDeviceRecord]) -> Result<(), StorageError> {
        for r in records {
            sqlx::query(
                "INSERT INTO to_device_messages (recipient_user, recipient_device, sender, event_type, content_json, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&r.recipient_user)
            .bind(&r.recipient_device)
            .bind(&r.sender)
            .bind(&r.event_type)
            .bind(&r.content_json)
            .bind(r.created_at)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn get_to_device_messages_impl(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<ToDeviceRecord>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, recipient_user, recipient_device, sender, event_type, content_json, created_at
             FROM to_device_messages
             WHERE recipient_user = ? AND recipient_device = ?
             ORDER BY id ASC",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| ToDeviceRecord {
                id: r.get("id"),
                recipient_user: r.get("recipient_user"),
                recipient_device: r.get("recipient_device"),
                sender: r.get("sender"),
                event_type: r.get("event_type"),
                content_json: r.get("content_json"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn delete_to_device_messages_impl(&self, ids: &[i64]) -> Result<(), StorageError> {
        for id in ids {
            sqlx::query("DELETE FROM to_device_messages WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        Ok(())
    }
}
