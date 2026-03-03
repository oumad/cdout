use serde::{Deserialize, Serialize};

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

/// Check if Claude Code credentials exist and are not expired
pub fn is_claude_code_available() -> bool {
    match read_claude_code_credentials() {
        Some(creds) => {
            if let Some(expires_at) = creds.expires_at {
                let now = chrono::Utc::now().timestamp();
                expires_at > now
            } else {
                // No expiry info — assume valid
                true
            }
        }
        None => false,
    }
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
