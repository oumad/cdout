use serde::{Deserialize, Serialize};

const ANTHROPIC_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const ANTHROPIC_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClaudeCodeCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CodexCredentials {
    pub api_key: String,
}

#[derive(Serialize, Clone)]
pub struct CliCredentialsStatus {
    pub claude_code: bool,
    pub codex: bool,
}

/// Read Claude Code credentials from ~/.claude/.credentials.json
pub fn read_claude_code_credentials() -> Option<ClaudeCodeCredentials> {
    let home = dirs::home_dir()?;
    let cred_path = home.join(".claude").join(".credentials.json");

    let content = std::fs::read_to_string(&cred_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Claude Code stores OAuth under claudeAiOauth
    let oauth = json.get("claudeAiOauth")?;
    let access_token = oauth.get("accessToken")?.as_str()?.to_string();
    let refresh_token = oauth.get("refreshToken").and_then(|v| v.as_str()).map(String::from);
    let expires_at = oauth.get("expiresAt").and_then(|v| v.as_i64());

    Some(ClaudeCodeCredentials {
        access_token,
        refresh_token,
        expires_at,
    })
}

/// Check if Claude Code credentials exist (may need refresh)
pub fn is_claude_code_available() -> bool {
    read_claude_code_credentials().is_some()
}

/// Refresh the Anthropic OAuth token using the refresh_token and update the credentials file.
pub async fn refresh_claude_code_token(refresh_token: &str) -> Result<ClaudeCodeCredentials, String> {
    let client = reqwest::Client::new();
    let res = client
        .post(ANTHROPIC_TOKEN_URL)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "client_id": ANTHROPIC_CLIENT_ID,
            "refresh_token": refresh_token,
        }))
        .send()
        .await
        .map_err(|e| format!("Token refresh request failed: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed: {}", text));
    }

    let data: serde_json::Value = res.json().await.map_err(|e| format!("Parse error: {}", e))?;
    let access_token = data.get("access_token").and_then(|v| v.as_str())
        .ok_or("Missing access_token in refresh response")?.to_string();
    let new_refresh = data.get("refresh_token").and_then(|v| v.as_str())
        .map(String::from);
    let expires_in = data.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
    // Store in milliseconds to match Claude Code's format
    let expires_at = chrono::Utc::now().timestamp_millis() + expires_in * 1000;

    // Update the credentials file
    if let Some(home) = dirs::home_dir() {
        let cred_path = home.join(".claude").join(".credentials.json");
        if let Ok(content) = std::fs::read_to_string(&cred_path) {
            if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(oauth) = json.get_mut("claudeAiOauth") {
                    oauth["accessToken"] = serde_json::Value::String(access_token.clone());
                    if let Some(ref rt) = new_refresh {
                        oauth["refreshToken"] = serde_json::Value::String(rt.clone());
                    }
                    oauth["expiresAt"] = serde_json::Value::Number(expires_at.into());
                    let _ = std::fs::write(&cred_path, serde_json::to_string_pretty(&json).unwrap_or_default());
                }
            }
        }
    }

    Ok(ClaudeCodeCredentials {
        access_token,
        refresh_token: new_refresh.or_else(|| Some(refresh_token.to_string())),
        expires_at: Some(expires_at),
    })
}

/// Get a valid (non-expired) Claude Code access token, refreshing if needed.
pub async fn get_valid_claude_code_credentials() -> Result<ClaudeCodeCredentials, String> {
    let creds = read_claude_code_credentials()
        .ok_or("Claude Code credentials not found. Run `claude` to log in.")?;

    // Check if token is expired (with 60s buffer)
    // Claude Code stores expiresAt in milliseconds
    if let Some(expires_at) = creds.expires_at {
        let expires_at_secs = if expires_at > 1_000_000_000_000 { expires_at / 1000 } else { expires_at };
        let now = chrono::Utc::now().timestamp();
        if expires_at_secs <= now + 60 {
            // Token expired or about to expire — try refresh
            if let Some(ref refresh_token) = creds.refresh_token {
                return refresh_claude_code_token(refresh_token).await;
            } else {
                return Err("Anthropic Auth Error: Token expired and no refresh token available. Run `claude` to refresh your session.".to_string());
            }
        }
    }

    Ok(creds)
}

/// Read Codex CLI credentials — checks env var first, then ~/.codex/auth.json
pub fn read_codex_credentials() -> Option<CodexCredentials> {
    // 1. Check env var
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        if !key.is_empty() {
            return Some(CodexCredentials { api_key: key });
        }
    }

    // 2. Check ~/.codex/auth.json
    let home = dirs::home_dir()?;
    let auth_path = home.join(".codex").join("auth.json");
    let content = std::fs::read_to_string(&auth_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    let api_key = json
        .get("api_key")
        .or_else(|| json.get("OPENAI_API_KEY"))
        .and_then(|v| v.as_str())?
        .to_string();

    if api_key.is_empty() {
        return None;
    }

    Some(CodexCredentials { api_key })
}

pub fn is_codex_available() -> bool {
    read_codex_credentials().is_some()
}

pub fn get_status() -> CliCredentialsStatus {
    CliCredentialsStatus {
        claude_code: is_claude_code_available(),
        codex: is_codex_available(),
    }
}
