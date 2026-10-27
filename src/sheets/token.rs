//! OAuth2 access tokens for a Google service account (JWT bearer grant,
//! RFC 7523). The credentials file is read lazily so the server starts even
//! when it is missing; only the Sheets-backed areas then report errors.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::{SheetsError, SCOPE};

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

pub enum TokenSource {
    ServiceAccount {
        path: PathBuf,
        cached: Mutex<Option<CachedToken>>,
    },
    /// Fixed token, for tests against a mock API.
    Static(String),
}

pub struct CachedToken {
    token: String,
    expires_at: u64,
}

#[derive(Deserialize)]
struct ServiceAccountKey {
    #[serde(rename = "type")]
    kind: Option<String>,
    client_email: String,
    private_key: String,
    token_uri: Option<String>,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl TokenSource {
    pub fn service_account(path: PathBuf) -> Self {
        Self::ServiceAccount {
            path,
            cached: Mutex::new(None),
        }
    }

    pub async fn access_token(&self, http: &reqwest::Client) -> Result<String, SheetsError> {
        let (path, cached) = match self {
            Self::Static(t) => return Ok(t.clone()),
            Self::ServiceAccount { path, cached } => (path, cached),
        };
        let mut guard = cached.lock().await;
        if let Some(c) = guard.as_ref() {
            if c.expires_at > now() + 60 {
                return Ok(c.token.clone());
            }
        }
        let fresh = fetch(path, http).await?;
        let token = fresh.token.clone();
        *guard = Some(fresh);
        Ok(token)
    }
}

async fn fetch(path: &PathBuf, http: &reqwest::Client) -> Result<CachedToken, SheetsError> {
    // Errors never include file contents, only what went wrong.
    let raw = tokio::fs::read(path).await.map_err(|e| {
        SheetsError::Credentials(format!("cannot read service account file: {}", e.kind()))
    })?;
    let key: ServiceAccountKey = serde_json::from_slice(&raw)
        .map_err(|_| SheetsError::Credentials("service account file is not valid JSON".into()))?;
    if key.kind.as_deref().is_some_and(|k| k != "service_account") {
        return Err(SheetsError::Credentials(
            "credentials file is not a service account".into(),
        ));
    }
    let token_uri = key.token_uri.as_deref().unwrap_or(DEFAULT_TOKEN_URI);
    let iat = now();
    let claims = Claims {
        iss: &key.client_email,
        scope: SCOPE,
        aud: token_uri,
        iat,
        exp: iat + 3600,
    };
    let signing_key = jsonwebtoken::EncodingKey::from_rsa_pem(key.private_key.as_bytes())
        .map_err(|_| SheetsError::Credentials("invalid service account private key".into()))?;
    let assertion = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
        &claims,
        &signing_key,
    )
    .map_err(|_| SheetsError::Credentials("cannot sign JWT".into()))?;

    let res = http
        .post(token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", assertion.as_str()),
        ])
        .send()
        .await
        .map_err(|e| SheetsError::Http(e.without_url().to_string()))?;
    if !res.status().is_success() {
        return Err(SheetsError::Status {
            status: res.status().as_u16(),
            message: "token exchange failed".into(),
        });
    }
    let body: TokenResponse = res
        .json()
        .await
        .map_err(|e| SheetsError::Http(e.without_url().to_string()))?;
    Ok(CachedToken {
        token: body.access_token,
        expires_at: iat + body.expires_in.unwrap_or(3600),
    })
}
