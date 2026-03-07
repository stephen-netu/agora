use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use agora_core::api::*;
use reqwest::Client;
use serde::de::DeserializeOwned;

/// HTTP client for the Agora / Matrix Client-Server API.
pub struct AgoraClient {
    http: Client,
    base_url: String,
    access_token: Option<String>,
    /// S-02: monotonic counter for idempotency txn IDs; no Uuid::new_v4()
    txn_counter: AtomicU64,
}

impl AgoraClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_owned(),
            access_token: None,
            txn_counter: AtomicU64::new(1),
        }
    }

    /// Generate a unique, deterministic transaction ID for idempotent Matrix requests.
    fn next_txn_id(&self) -> String {
        format!("{:016x}", self.txn_counter.fetch_add(1, Ordering::Relaxed))
    }

    pub fn set_token(&mut self, token: String) {
        self.access_token = Some(token);
    }

    pub fn token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }

    pub fn server_name(&self) -> &str {
        self.base_url
            .strip_prefix("http://")
            .or_else(|| self.base_url.strip_prefix("https://"))
            .unwrap_or(&self.base_url)
    }

    fn url(&self, path: &str) -> String {
        format!("{}/_matrix/client{}", self.base_url, path)
    }

    fn media_url(&self, path: &str) -> String {
        format!("{}/_matrix/media{}", self.base_url, path)
    }

    fn auth_header(&self) -> Result<String, CliClientError> {
        self.access_token
            .as_ref()
            .map(|t| format!("Bearer {t}"))
            .ok_or(CliClientError::NotLoggedIn)
    }

    async fn parse_response<T: DeserializeOwned>(
        resp: reqwest::Response,
    ) -> Result<T, CliClientError> {
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CliClientError::Server {
                status: status.as_u16(),
                body,
            });
        }
        resp.json().await.map_err(CliClientError::Http)
    }

    // -- Auth ----------------------------------------------------------------

    pub async fn register(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<RegisterResponse, CliClientError> {
        let resp = self
            .http
            .post(self.url("/v3/register"))
            .json(&serde_json::json!({
                "username": username,
                "password": password,
            }))
            .send()
            .await
            .map_err(CliClientError::Http)?;

        let result: RegisterResponse = Self::parse_response(resp).await?;
        self.access_token = Some(result.access_token.clone());
        Ok(result)
    }

    pub async fn login(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<LoginResponse, CliClientError> {
        let resp = self
            .http
            .post(self.url("/v3/login"))
            .json(&serde_json::json!({
                "type": "m.login.password",
                "user": username,
                "password": password,
            }))
            .send()
            .await
            .map_err(CliClientError::Http)?;

        let result: LoginResponse = Self::parse_response(resp).await?;
        self.access_token = Some(result.access_token.clone());
        Ok(result)
    }

    pub async fn logout(&mut self) -> Result<(), CliClientError> {
        let auth = self.auth_header()?;
        self.http
            .post(self.url("/v3/logout"))
            .header("Authorization", auth)
            .send()
            .await
            .map_err(CliClientError::Http)?;
        self.access_token = None;
        Ok(())
    }

    // -- Rooms ---------------------------------------------------------------

    pub async fn create_room(
        &self,
        name: Option<&str>,
        topic: Option<&str>,
    ) -> Result<CreateRoomResponse, CliClientError> {
        self.create_room_with_options(name, topic, None).await
    }

    pub async fn create_space(
        &self,
        name: Option<&str>,
        topic: Option<&str>,
    ) -> Result<CreateRoomResponse, CliClientError> {
        self.create_room_with_options(
            name,
            topic,
            Some(serde_json::json!({ "type": "m.space" })),
        )
        .await
    }

    async fn create_room_with_options(
        &self,
        name: Option<&str>,
        topic: Option<&str>,
        creation_content: Option<serde_json::Value>,
    ) -> Result<CreateRoomResponse, CliClientError> {
        let auth = self.auth_header()?;
        let mut body = serde_json::Map::new();
        if let Some(n) = name {
            body.insert("name".into(), serde_json::Value::String(n.to_owned()));
        }
        if let Some(t) = topic {
            body.insert("topic".into(), serde_json::Value::String(t.to_owned()));
        }
        if let Some(cc) = creation_content {
            body.insert("creation_content".into(), cc);
        }

        let resp = self
            .http
            .post(self.url("/v3/createRoom"))
            .header("Authorization", auth)
            .json(&body)
            .send()
            .await
            .map_err(CliClientError::Http)?;

        Self::parse_response(resp).await
    }

    pub async fn join_room(&self, room_id: &str) -> Result<JoinRoomResponse, CliClientError> {
        let auth = self.auth_header()?;
        let resp = self
            .http
            .post(self.url(&format!("/v3/join/{}", urlencoding(room_id))))
            .header("Authorization", auth)
            .send()
            .await
            .map_err(CliClientError::Http)?;

        Self::parse_response(resp).await
    }

    pub async fn leave_room(&self, room_id: &str) -> Result<(), CliClientError> {
        let auth = self.auth_header()?;
        self.http
            .post(self.url(&format!(
                "/v3/rooms/{}/leave",
                urlencoding(room_id)
            )))
            .header("Authorization", auth)
            .send()
            .await
            .map_err(CliClientError::Http)?;
        Ok(())
    }

    pub async fn send_event(
        &self,
        room_id: &str,
        event_type: &str,
        content: serde_json::Value,
    ) -> Result<SendEventResponse, CliClientError> {
        let auth = self.auth_header()?;
        let txn_id = self.next_txn_id();
        let resp = self
            .http
            .put(self.url(&format!(
                "/v3/rooms/{}/send/{}/{}",
                urlencoding(room_id),
                urlencoding(event_type),
                txn_id,
            )))
            .header("Authorization", auth)
            .json(&content)
            .send()
            .await
            .map_err(CliClientError::Http)?;

        Self::parse_response(resp).await
    }

    pub async fn get_messages(
        &self,
        room_id: &str,
        limit: u64,
    ) -> Result<MessagesResponse, CliClientError> {
        let auth = self.auth_header()?;
        let resp = self
            .http
            .get(self.url(&format!(
                "/v3/rooms/{}/messages",
                urlencoding(room_id)
            )))
            .header("Authorization", auth)
            .query(&[("limit", limit.to_string()), ("dir", "b".to_owned())])
            .send()
            .await
            .map_err(CliClientError::Http)?;

        Self::parse_response(resp).await
    }

    // -- State events --------------------------------------------------------

    pub async fn set_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        content: serde_json::Value,
    ) -> Result<SendEventResponse, CliClientError> {
        let auth = self.auth_header()?;
        let resp = self
            .http
            .put(self.url(&format!(
                "/v3/rooms/{}/state/{}/{}",
                urlencoding(room_id),
                urlencoding(event_type),
                urlencoding(state_key),
            )))
            .header("Authorization", auth)
            .json(&content)
            .send()
            .await
            .map_err(CliClientError::Http)?;

        Self::parse_response(resp).await
    }

    // -- Hierarchy -----------------------------------------------------------

    pub async fn get_hierarchy(
        &self,
        space_id: &str,
    ) -> Result<HierarchyResponse, CliClientError> {
        let auth = self.auth_header()?;
        let resp = self
            .http
            .get(self.url(&format!(
                "/v1/rooms/{}/hierarchy",
                urlencoding(space_id)
            )))
            .header("Authorization", auth)
            .send()
            .await
            .map_err(CliClientError::Http)?;

        Self::parse_response(resp).await
    }

    // -- Sync ----------------------------------------------------------------

    pub async fn sync(
        &self,
        since: Option<&str>,
        timeout: u64,
    ) -> Result<SyncResponse, CliClientError> {
        let auth = self.auth_header()?;
        let mut query = vec![("timeout", timeout.to_string())];
        if let Some(s) = since {
            query.push(("since", s.to_owned()));
        }

        let resp = self
            .http
            .get(self.url("/v3/sync"))
            .header("Authorization", auth)
            .query(&query)
            .send()
            .await
            .map_err(CliClientError::Http)?;

        Self::parse_response(resp).await
    }

    // -- Media ---------------------------------------------------------------

    /// Upload a file and return its `mxc://` content URI.
    pub async fn upload_file(&self, path: &Path) -> Result<String, CliClientError> {
        let auth = self.auth_header()?;
        let data = tokio::fs::read(path).await.map_err(|e| {
            CliClientError::Io(format!("failed to read {}: {e}", path.display()))
        })?;

        let content_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());

        let mut url = self.media_url("/v3/upload");
        if let Some(ref name) = filename {
            url = format!("{}?filename={}", url, urlencoding(name));
        }

        let resp = self
            .http
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", content_type)
            .body(data)
            .send()
            .await
            .map_err(CliClientError::Http)?;

        let result: MediaUploadResponse = Self::parse_response(resp).await?;
        Ok(result.content_uri)
    }

    /// Download media by `mxc://` URI to a local file path.
    pub async fn download_file(
        &self,
        mxc_uri: &str,
        dest: &Path,
    ) -> Result<(), CliClientError> {
        let auth = self.auth_header()?;
        let stripped = mxc_uri
            .strip_prefix("mxc://")
            .ok_or_else(|| CliClientError::Io("invalid mxc:// URI".into()))?;

        let url = self.media_url(&format!("/v3/download/{stripped}"));

        let resp = self
            .http
            .get(&url)
            .header("Authorization", auth)
            .send()
            .await
            .map_err(CliClientError::Http)?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CliClientError::Server {
                status: status.as_u16(),
                body,
            });
        }

        let bytes = resp.bytes().await.map_err(CliClientError::Http)?;
        tokio::fs::write(dest, &bytes).await.map_err(|e| {
            CliClientError::Io(format!("failed to write {}: {e}", dest.display()))
        })?;

        Ok(())
    }

    // -- Sigchain ------------------------------------------------------------

    fn agora_url(&self, path: &str) -> String {
        format!("{}/_agora{}", self.base_url, path)
    }

    /// Publish a sigchain link to the server.
    ///
    /// Returns `(seqno, canonical_hash_hex)` on success. Errors are non-fatal
    /// in the send flow — callers should log and proceed rather than aborting.
    pub async fn publish_sigchain_link(
        &self,
        agent_id_hex: &str,
        link: &agora_crypto::SigchainLink,
    ) -> Result<(u64, String), CliClientError> {
        let auth = self.auth_header()?;
        let resp = self
            .http
            .put(self.agora_url(&format!("/sigchain/{agent_id_hex}")))
            .header("Authorization", auth)
            .json(link)
            .send()
            .await
            .map_err(CliClientError::Http)?;

        let result: serde_json::Value = Self::parse_response(resp).await?;
        let seqno = result["seqno"].as_u64().ok_or_else(|| CliClientError::Server {
            status: 0,
            body: "server response missing 'seqno' field".to_owned(),
        })?;
        let hash = result["canonical_hash"]
            .as_str()
            .ok_or_else(|| CliClientError::Server {
                status: 0,
                body: "server response missing 'canonical_hash' field".to_owned(),
            })?
            .to_owned();
        Ok((seqno, hash))
    }

    // -- Presence ------------------------------------------------------------
    // IMPLEMENTATION_REQUIRED: presence methods - wire up to CLI commands

    // -- Sigchain ------------------------------------------------------------
}

fn urlencoding(s: &str) -> String {
    s.replace('!', "%21")
        .replace('#', "%23")
        .replace('$', "%24")
        .replace('@', "%40")
        .replace(':', "%3A")
}

#[derive(Debug, thiserror::Error)]
pub enum CliClientError {
    #[error("HTTP error: {0}")]
    Http(reqwest::Error),
    #[error("server error ({status}): {body}")]
    Server { status: u16, body: String },
    #[error("not logged in — use `agora login` first")]
    NotLoggedIn,
    #[error("I/O error: {0}")]
    Io(String),
}
