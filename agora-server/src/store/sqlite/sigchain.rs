//! SQLite implementation of sigchain storage.

use sqlx::Row;

use super::{SigchainLinkRecord, SqliteStore, StorageError};

impl SqliteStore {
    pub(super) async fn store_sigchain_link_impl(
        &self,
        record: &SigchainLinkRecord,
    ) -> Result<(), StorageError> {
        let result = sqlx::query(
            "INSERT INTO sigchain_links
                (agent_id, seqno, link_json, canonical_hash, link_type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(record.agent_id.as_slice())
        .bind(record.seqno as i64)
        .bind(&record.link_json)
        .bind(record.canonical_hash.as_slice())
        .bind(&record.link_type)
        .bind(record.created_at as i64)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                Err(StorageError::Conflict(format!(
                    "sigchain link (agent_id, seqno={}) already exists",
                    record.seqno
                )))
            }
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    pub(super) async fn get_sigchain_impl(
        &self,
        agent_id: &[u8; 32],
    ) -> Result<Vec<SigchainLinkRecord>, StorageError> {
        let rows = sqlx::query(
            "SELECT agent_id, seqno, link_json, canonical_hash, link_type, created_at
             FROM sigchain_links
             WHERE agent_id = ?1
             ORDER BY seqno ASC",
        )
        .bind(agent_id.as_slice())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        rows.iter().map(row_to_record).collect()
    }

    pub(super) async fn get_sigchain_since_impl(
        &self,
        agent_id: &[u8; 32],
        since_seqno: u64,
    ) -> Result<Vec<SigchainLinkRecord>, StorageError> {
        let rows = sqlx::query(
            "SELECT agent_id, seqno, link_json, canonical_hash, link_type, created_at
             FROM sigchain_links
             WHERE agent_id = ?1 AND seqno > ?2
             ORDER BY seqno ASC",
        )
        .bind(agent_id.as_slice())
        .bind(since_seqno as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        rows.iter().map(row_to_record).collect()
    }
}

fn row_to_record(row: &sqlx::sqlite::SqliteRow) -> Result<SigchainLinkRecord, StorageError> {
    let agent_id_bytes: Vec<u8> = row.get("agent_id");
    let agent_id: [u8; 32] = agent_id_bytes.try_into().map_err(|_| {
        StorageError::Database("agent_id column is not 32 bytes".into())
    })?;

    let canonical_hash_bytes: Vec<u8> = row.get("canonical_hash");
    let canonical_hash: [u8; 32] = canonical_hash_bytes.try_into().map_err(|_| {
        StorageError::Database("canonical_hash column is not 32 bytes".into())
    })?;

    let seqno: i64 = row.get("seqno");
    let created_at: i64 = row.get("created_at");

    Ok(SigchainLinkRecord {
        agent_id,
        seqno: seqno as u64,
        link_json: row.get("link_json"),
        canonical_hash,
        link_type: row.get("link_type"),
        created_at: created_at as u64,
    })
}
