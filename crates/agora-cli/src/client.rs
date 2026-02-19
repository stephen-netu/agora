use agora_core::api::*;
use reqwest::Client;
use serde::de::DeserializeOwned;

/// HTTP client for the Agora / Matrix Client-Server API.
pub struct AgoraClient {
    http: Client,
    base_url: String,
    access_token: Option<String>,
}

impl AgoraClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_owned(),
            access_token: None,
        }
    }

    pub fn set_token(&mut self, token: String) {
        self.access_token = Some(token);
    }

    pub fn token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }

    fn url(&self, path: &str) -> String {
        format!("{}/_matrix/client{}", self.base_url, path)
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
        let auth = self.auth_header()?;
        let mut body = serde_json::Map::new();
        if let Some(n) = name {
            body.insert("name".into(), serde_json::Value::String(n.to_owned()));
        }
        if let Some(t) = topic {
            body.insert("topic".into(), serde_json::Value::String(t.to_owned()));
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

    // -- Messages ------------------------------------------------------------

    pub async fn send_message(
        &self,
        room_id: &str,
        body: &str,
    ) -> Result<SendEventResponse, CliClientError> {
        let auth = self.auth_header()?;
        let txn_id = uuid::Uuid::new_v4().simple().to_string();
        let resp = self
            .http
            .put(self.url(&format!(
                "/v3/rooms/{}/send/m.room.message/{}",
                urlencoding(room_id),
                txn_id,
            )))
            .header("Authorization", auth)
            .json(&serde_json::json!({
                "msgtype": "m.text",
                "body": body,
            }))
            .send()
            .await
            .map_err(CliClientError::Http)?;

        Self::parse_response(resp).await
    }

    pub async fn send_event(
        &self,
        room_id: &str,
        event_type: &str,
        content: serde_json::Value,
    ) -> Result<SendEventResponse, CliClientError> {
        let auth = self.auth_header()?;
        let txn_id = uuid::Uuid::new_v4().simple().to_string();
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
}
