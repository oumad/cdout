use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use tiny_http::{Response, Server};

// Constants from pi-ai research
const CLIENT_ID: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "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"; // Hardcoded as per pi-ai findings
const REDIRECT_URI: &str = "http://localhost:51121/oauth-callback";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/cclog",
    "https://www.googleapis.com/auth/experimentsandconfigs",
];

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AntigravityCredentials {
    pub refresh_token: String,
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
    pub project_id: Option<String>,
    pub email: Option<String>,
}

#[derive(Deserialize, Debug)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Deserialize, Debug)]
struct UserInfo {
    email: String,
}

fn get_credentials_path() -> PathBuf {
    // Store in same dir as config
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("shuttle-io"); // AppData/Roaming/shuttle-io
    std::fs::create_dir_all(&path).ok();
    path.push("antigravity_credentials.json");
    path
}

fn generate_pkce() -> (String, String) {
    let mut rng = rand::thread_rng();
    let mut verifier_bytes = [0u8; 32];
    rng.fill_bytes(&mut verifier_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    (verifier, challenge)
}

pub async fn perform_login() -> Result<AntigravityCredentials, String> {
    let (verifier, challenge) = generate_pkce();
    let verifier_for_callback = verifier.clone();

    // Start local server
    let server = Server::http("127.0.0.1:51121")
        .map_err(|e| format!("Failed to start callback server: {}", e))?;

    // Construct Auth URL
    let scopes_joined = SCOPES.join(" ");
    let auth_url = format!(
        "{}?client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}&access_type=offline&prompt=consent",
        AUTH_URL, CLIENT_ID, REDIRECT_URI, urlencoding::encode(&scopes_joined), challenge, verifier
    );

    // Open browser
    if let Err(e) = webbrowser::open(&auth_url) {
        return Err(format!("Failed to open browser: {}", e));
    }

    // Wait for callback
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        // Just handle one request
        if let Ok(request) = server.recv() {
            let url = request.url().to_string();
            if url.contains("/oauth-callback") {
                // ... parsing logic ...
                let query_part = url.split('?').nth(1).unwrap_or("");
                let mut code = None;
                let mut state = None;
                for pair in query_part.split('&') {
                    let parts: Vec<&str> = pair.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        if parts[0] == "code" {
                            code = Some(parts[1].to_string());
                        }
                        if parts[0] == "state" {
                            state = Some(parts[1].to_string());
                        }
                    }
                }

                if let Some(c) = code {
                    // Check state against verifier
                    if state.as_deref() == Some(&verifier_for_callback) {
                        let _ = tx.send(Ok(c));
                        let _ = request.respond(
                            Response::from_string("Authentication Successful! Return to app.")
                                .with_status_code(200),
                        );
                    } else {
                        let _ = tx.send(Err("State verification failed".to_string()));
                        let _ = request
                            .respond(Response::from_string("State mismatch").with_status_code(400));
                    }
                } else {
                    let _ = tx.send(Err("No code found".to_string()));
                    let _ = request
                        .respond(Response::from_string("Invalid request").with_status_code(400));
                }
            }
        }
    });

    let code = rx.recv().map_err(|_| "Auth server channel closed")??;

    // Exchange Code
    let client = Client::new();
    let res = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("code", &code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", &verifier),
        ])
        .send()
        .await
        .map_err(|e| format!("Token request failed: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {}", text));
    }

    let token_data: TokenResponse = res
        .json()
        .await
        .map_err(|e| format!("Parse token error: {}", e))?;

    // Refresh token is required for offline access
    if token_data.refresh_token.is_none() {
        // Warning: Might happen if user already granted access and we got a token without consent screen?
        // But we force prompt=consent so we should get it.
    }

    // Get User Info (Email)
    let email_response = client
        .get("https://www.googleapis.com/oauth2/v1/userinfo?alt=json")
        .bearer_auth(&token_data.access_token)
        .send()
        .await;

    let mut email = None;
    if let Ok(resp) = email_response {
        if let Ok(info) = resp.json::<UserInfo>().await {
            email = Some(info.email);
        }
    }

    // Construct final credentials
    let creds = AntigravityCredentials {
        refresh_token: token_data.refresh_token.clone().unwrap_or_default(), // Handle missing RT gracefully-ish
        access_token: token_data.access_token,
        expires_at: Utc::now() + Duration::seconds(token_data.expires_in - 300), // Buffer 5 mins
        project_id: Some("rising-fact-p41fc".to_string()),
        email,
    };

    save_credentials(&creds)?;

    Ok(creds)
}

pub fn save_credentials(creds: &AntigravityCredentials) -> Result<(), String> {
    let path = get_credentials_path();
    let json = serde_json::to_string_pretty(creds).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn load_credentials() -> Option<AntigravityCredentials> {
    let path = get_credentials_path();
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub async fn get_valid_token() -> Result<String, String> {
    let mut creds = load_credentials().ok_or("No credentials found. Please login.")?;

    if Utc::now() < creds.expires_at {
        return Ok(creds.access_token);
    }

    // Refresh
    let client = Client::new();
    let res = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("refresh_token", &creds.refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| format!("Refresh request failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Token refresh failed: {}", res.status()));
    }

    let token_data: TokenResponse = res
        .json()
        .await
        .map_err(|e| format!("Parse refresh error: {}", e))?;

    creds.access_token = token_data.access_token;
    creds.expires_at = Utc::now() + Duration::seconds(token_data.expires_in - 300);
    if let Some(rt) = token_data.refresh_token {
        creds.refresh_token = rt;
    }

    save_credentials(&creds)?;

    Ok(creds.access_token)
}
