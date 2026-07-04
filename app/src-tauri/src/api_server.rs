//! Local HTTP API server for the browser extension.
//! Runs on localhost:7890 with token-based auth.
//! Provides read-only access to vault entries filtered by domain.

use crate::commands::AppState;
use serde::Serialize;
#[allow(unused_imports)]
use std::io::Read as _;
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, Response, Server};

const PORT: u16 = 7890;

#[derive(Serialize)]
struct StatusResponse {
    unlocked: bool,
    version: &'static str,
}

#[derive(Serialize)]
struct EntryItem {
    id: String,
    name: String,
    username: String,
    password: String,
    url: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Start the local API server on a background thread.
/// The server shares the AppState with Tauri via Arc.
pub fn start_api_server(state: Arc<AppState>) {
    thread::spawn(move || {
        let server = match Server::http(format!("127.0.0.1:{}", PORT)) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to start API server: {}", e);
                return;
            }
        };

        log::info!("Vault API server running on http://127.0.0.1:{}", PORT);

        for mut request in server.incoming_requests() {
            // CORS headers for extension access
            let cors_origin = Header::from_bytes(
                &b"Access-Control-Allow-Origin"[..],
                &b"*"[..],
            )
            .unwrap();
            let cors_headers = Header::from_bytes(
                &b"Access-Control-Allow-Headers"[..],
                &b"Authorization, Content-Type"[..],
            )
            .unwrap();
            let cors_methods = Header::from_bytes(
                &b"Access-Control-Allow-Methods"[..],
                &b"GET, POST, OPTIONS"[..],
            )
            .unwrap();
            let content_type = Header::from_bytes(
                &b"Content-Type"[..],
                &b"application/json"[..],
            )
            .unwrap();

            // Handle CORS preflight
            if request.method().as_str() == "OPTIONS" {
                let response = Response::empty(204)
                    .with_header(cors_origin)
                    .with_header(cors_headers)
                    .with_header(cors_methods);
                let _ = request.respond(response);
                continue;
            }

            // Check auth token
            let token_guard = state.api_token.lock().unwrap();
            let expected_token = token_guard.clone();
            drop(token_guard);

            if let Some(ref token) = expected_token {
                let auth_value: Option<String> = request
                    .headers()
                    .iter()
                    .find(|h| {
                        let field = h.field.to_string();
                        field.eq_ignore_ascii_case("Authorization")
                    })
                    .map(|h| h.value.to_string());

                let authorized = match auth_value {
                    Some(val) => {
                        val == format!("Bearer {}", token) || val == *token
                    }
                    None => false,
                };

                if !authorized && request.url() != "/status" {
                    let body = serde_json::to_string(&ErrorResponse {
                        error: "Unauthorized".to_string(),
                    })
                    .unwrap();
                    let response = Response::from_string(body)
                        .with_status_code(401)
                        .with_header(content_type)
                        .with_header(cors_origin);
                    let _ = request.respond(response);
                    continue;
                }
            }

            // Route requests
            let url = request.url().to_string();
            let method = request.method().as_str().to_string();

            match (method.as_str(), url.as_str()) {
                (_, "/status") => {
                    let unlocked = state.vault_data.lock().unwrap().is_some();
                    let body = serde_json::to_string(&StatusResponse {
                        unlocked,
                        version: "0.1.0",
                    })
                    .unwrap();
                    let response = Response::from_string(body)
                        .with_header(content_type)
                        .with_header(cors_origin);
                    let _ = request.respond(response);
                }
                ("GET", _) if url.starts_with("/entries") => {
                    // Parse domain query param: /entries?domain=github.com
                    let domain = url
                        .split('?')
                        .nth(1)
                        .and_then(|q| {
                            q.split('&')
                                .find(|p| p.starts_with("domain="))
                                .map(|p| p.trim_start_matches("domain=").to_string())
                        })
                        .unwrap_or_default();

                    let guard = state.vault_data.lock().unwrap();
                    match guard.as_ref() {
                        None => {
                            let body = serde_json::to_string(&ErrorResponse {
                                error: "Vault is locked".to_string(),
                            })
                            .unwrap();
                            let response = Response::from_string(body)
                                .with_status_code(403)
                                .with_header(content_type)
                                .with_header(cors_origin);
                            let _ = request.respond(response);
                        }
                        Some(data) => {
                            let entries: Vec<EntryItem> = data
                                .entries
                                .values()
                                .filter(|e| {
                                    if domain.is_empty() {
                                        true
                                    } else {
                                        e.url.to_lowercase().contains(&domain.to_lowercase())
                                            || e.name.to_lowercase().contains(&domain.to_lowercase())
                                    }
                                })
                                .map(|e| EntryItem {
                                    id: e.id.clone(),
                                    name: e.name.clone(),
                                    username: e.username.clone(),
                                    password: e.password.clone(),
                                    url: e.url.clone(),
                                })
                                .collect();

                            let body = serde_json::to_string(&entries).unwrap();
                            let response = Response::from_string(body)
                                .with_header(content_type)
                                .with_header(cors_origin);
                            let _ = request.respond(response);
                        }
                    }
                }
                ("POST", "/entries") => {
                    // Add a new entry from the browser extension
                    let mut body_buf = String::new();
                    request.as_reader().read_to_string(&mut body_buf).ok();

                    #[derive(serde::Deserialize)]
                    struct NewEntry {
                        name: Option<String>,
                        username: String,
                        password: String,
                        url: Option<String>,
                    }

                    match serde_json::from_str::<NewEntry>(&body_buf) {
                        Err(_) => {
                            let body = serde_json::to_string(&ErrorResponse {
                                error: "Invalid JSON body".to_string(),
                            }).unwrap();
                            let response = Response::from_string(body)
                                .with_status_code(400)
                                .with_header(content_type)
                                .with_header(cors_origin);
                            let _ = request.respond(response);
                        }
                        Ok(input) => {
                            let mut guard = state.vault_data.lock().unwrap();
                            match guard.as_mut() {
                                None => {
                                    let body = serde_json::to_string(&ErrorResponse {
                                        error: "Vault is locked".to_string(),
                                    }).unwrap();
                                    let response = Response::from_string(body)
                                        .with_status_code(403)
                                        .with_header(content_type)
                                        .with_header(cors_origin);
                                    let _ = request.respond(response);
                                }
                                Some(data) => {
                                    let entry_url = input.url.unwrap_or_default();
                                    let entry_name = input.name.unwrap_or_else(|| {
                                        entry_url.replace("https://", "")
                                            .replace("http://", "")
                                            .split('/')
                                            .next()
                                            .unwrap_or("Saved from browser")
                                            .to_string()
                                    });

                                    // Check for duplicate: same username + same domain
                                    let is_duplicate = data.entries.values().any(|e| {
                                        e.username == input.username && (
                                            e.url.to_lowercase().contains(&entry_url.to_lowercase()) ||
                                            entry_url.to_lowercase().contains(&e.url.to_lowercase()) ||
                                            e.name.to_lowercase() == entry_name.to_lowercase()
                                        )
                                    });

                                    if is_duplicate {
                                        let body = serde_json::to_string(&serde_json::json!({"id": "", "saved": false, "reason": "duplicate"})).unwrap();
                                        let response = Response::from_string(body)
                                            .with_status_code(200)
                                            .with_header(content_type)
                                            .with_header(cors_origin);
                                        let _ = request.respond(response);
                                        return;
                                    }

                                    let id = uuid::Uuid::new_v4().to_string();
                                    let entry = crate::vault::VaultEntry {
                                        id: id.clone(),
                                        name: entry_name,
                                        username: input.username,
                                        password: input.password,
                                        url: entry_url,
                                        notes: String::new(),
                                        category: "Saved from browser".to_string(),
                                        created_at: chrono::Utc::now().to_rfc3339(),
                                        updated_at: String::new(),
                                    };
                                    data.entries.insert(id.clone(), entry);

                                    // Save to disk
                                    let pw_guard = state.master_password.lock().unwrap();
                                    if let Some(pw) = pw_guard.as_ref() {
                                        let _ = crate::vault::save_vault_with_password(data, pw);
                                    }

                                    let body = serde_json::to_string(&serde_json::json!({"id": id, "saved": true})).unwrap();
                                    let response = Response::from_string(body)
                                        .with_status_code(201)
                                        .with_header(content_type)
                                        .with_header(cors_origin);
                                    let _ = request.respond(response);
                                }
                            }
                        }
                    }
                }
                _ => {
                    let body = serde_json::to_string(&ErrorResponse {
                        error: "Not found".to_string(),
                    })
                    .unwrap();
                    let response = Response::from_string(body)
                        .with_status_code(404)
                        .with_header(content_type)
                        .with_header(cors_origin);
                    let _ = request.respond(response);
                }
            }
        }
    });
}
