use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

use agora_core::api::{MediaConfigResponse, MediaUploadResponse};

use crate::error::ApiError;
use crate::state::AppState;
use crate::store::MediaRecord;

use super::AuthUser;

#[derive(Deserialize)]
pub struct UploadQuery {
    #[serde(default)]
    pub filename: Option<String>,
}

pub async fn upload(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<MediaUploadResponse>, ApiError> {
    let size = body.len() as u64;
    if size > state.max_upload_bytes {
        return Err(ApiError::too_large(format!(
            "upload size {} exceeds maximum {}",
            size, state.max_upload_bytes
        )));
    }

    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();

    let media_id = uuid::Uuid::new_v4().simple().to_string();

    let prefix = &media_id[..2];
    let dir = state.media_path.join(prefix);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        tracing::error!("failed to create media dir: {e}");
        ApiError::unknown("media storage error")
    })?;

    let file_path = dir.join(&media_id);
    let mut file = tokio::fs::File::create(&file_path).await.map_err(|e| {
        tracing::error!("failed to create media file: {e}");
        ApiError::unknown("media storage error")
    })?;
    file.write_all(&body).await.map_err(|e| {
        tracing::error!("failed to write media file: {e}");
        ApiError::unknown("media storage error")
    })?;
    file.flush().await.ok();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let record = MediaRecord {
        media_id: media_id.clone(),
        server_name: state.server_name.clone(),
        uploader: user_id.to_string(),
        content_type,
        file_size: size as i64,
        upload_name: query.filename,
        created_at: now,
    };

    state.store.store_media(&record).await?;

    let content_uri = format!("mxc://{}/{}", state.server_name, media_id);
    Ok(Json(MediaUploadResponse { content_uri }))
}

pub async fn download(
    State(state): State<AppState>,
    Path(params): Path<DownloadPath>,
) -> Result<Response, ApiError> {
    let record = state
        .store
        .get_media(&params.server_name, &params.media_id)
        .await?
        .ok_or_else(|| ApiError::not_found("media not found"))?;

    if params.server_name != state.server_name {
        return Err(ApiError::not_found("remote media not supported"));
    }

    let prefix = &record.media_id[..2];
    let file_path = state.media_path.join(prefix).join(&record.media_id);

    let file = tokio::fs::File::open(&file_path).await.map_err(|e| {
        tracing::error!("failed to open media file: {e}");
        ApiError::not_found("media file missing from storage")
    })?;

    let stream = ReaderStream::new(file);

    let filename = params
        .filename
        .as_deref()
        .or(record.upload_name.as_deref())
        .unwrap_or("download");

    let disposition = format!("inline; filename=\"{}\"", filename.replace('"', "\\\""));

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, &record.content_type)
        .header(CONTENT_DISPOSITION, disposition)
        .body(Body::from_stream(stream))
        .map_err(|e| ApiError::unknown(format!("failed to build response: {e}")))?;

    Ok(response)
}

#[derive(Deserialize)]
pub struct DownloadPath {
    pub server_name: String,
    pub media_id: String,
    pub filename: Option<String>,
}

pub async fn config(
    State(state): State<AppState>,
) -> Json<MediaConfigResponse> {
    Json(MediaConfigResponse {
        m_upload_size: Some(state.max_upload_bytes),
    })
}
