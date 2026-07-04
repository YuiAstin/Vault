//! Local HTTP API server for the browser extension.
//! Runs on localhost:7890 with tokenless access (localhost = trusted).
//! Rate-limited: if requests come inhumanly fast, locks out and requires pairing.

use crate::commands::AppState;
use serde::Serialize;
#[allow(unused_imports)]
use std::io::Read as _;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Header, Response, Server};

const PORT: u16 = 7890;
const RATE_LIMIT_MAX: usize = 5; // max credential fetches
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(10); // in this window

#[derive(Serialize)]
struct StatusResponse {
    unlocked: bool,
    version: &'static str,
    paired: bool,
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

#[derive(Serialize)]
struct RateLimitResponse {
    error: String,
    code: String,
    pairing_required: bool,
}

/// Rate limiter state
struct RateLimiter {
    /// Timestamps of recent credential-fetching requests
    timestamps: Vec<Instant>,
    /// If locked out, when the lockout started
    locked_until: Option<Instant>,
    /// Active pairing code (6 digits), set when lockout triggers
    pairing_code: Option<String>,
    /// Whether a client has successfully paired (resets on lockout)
    paired: bool,
    /// Entries that were served before lockout (potentially compromised)
    exposed_entries: Vec<ExposedEntry>,
}

#[derive(Clone, Serialize)]
struct ExposedEntry {
    name: String,
    username: String,
    url: String,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            timestamps: Vec::new(),
            locked_until: None,
            pairing_code: None,
            paired: true, // Start paired (no challenge needed initially)
            exposed_entries: Vec::new(),
        }
    }

    /// Record a credential access. Returns true if allowed, false if rate limited.
    fn check_and_record(&mut self) -> bool {
        let now = Instant::now();

        // If locked out, stay locked until paired
        if self.locked_until.is_some() {
            return false;
        }

        // Prune old timestamps outside the window
        self.timestamps.retain(|t| now.duration_since(*t) < RATE_LIMIT_WINDOW);

        // Check rate
        if self.timestamps.len() >= RATE_LIMIT_MAX {
            // Trigger lockout — stays until pairing
            self.locked_until = Some(now);
            self.paired = false;
            self.pairing_code = Some(generate_pairing_code());
            return false;
        }

        self.timestamps.push(now);
        true
    }

    fn is_locked(&self) -> bool {
        self.locked_until.is_some()
    }

    fn get_pairing_code(&self) -> Option<&String> {
        self.pairing_code.as_ref()
    }

    fn try_pair(&mut self, code: &str) -> bool {
        if let Some(ref expected) = self.pairing_code {
            if code == expected {
                self.paired = true;
                self.locked_until = None;
                self.pairing_code = None;
                self.timestamps.clear();
                // Don't clear exposed_entries — keep them for the alert
                return true;
            }
        }
        false
    }

    fn record_exposed(&mut self, entries: &[ExposedEntry]) {
        for entry in entries {
            // Avoid duplicates
            if !self.exposed_entries.iter().any(|e| e.username == entry.username && e.url == entry.url) {
                self.exposed_entries.push(entry.clone());
            }
        }
    }

    fn get_exposed(&self) -> &[ExposedEntry] {
        &self.exposed_entries
    }

    fn clear_exposed(&mut self) {
        self.exposed_entries.clear();
    }
}

fn generate_pairing_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1000000))
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

        let rate_limiter = Arc::new(Mutex::new(RateLimiter::new()));

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
                &b"Content-Type"[..],
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

            // Route requests
            let url = request.url().to_string();
            let method = request.method().as_str().to_string();

            match (method.as_str(), url.as_str()) {
                (_, "/status") => {
                    let unlocked = state.vault_data.lock().unwrap().is_some();
                    let rl = rate_limiter.lock().unwrap();
                    let body = serde_json::to_string(&StatusResponse {
                        unlocked,
                        version: "0.3.0",
                        paired: rl.paired,
                    })
                    .unwrap();
                    let response = Response::from_string(body)
                        .with_header(content_type)
                        .with_header(cors_origin);
                    let _ = request.respond(response);
                }
                ("POST", "/pair") => {
                    // Pairing challenge: client sends {"code": "123456"}
                    let mut body_buf = String::new();
                    request.as_reader().read_to_string(&mut body_buf).ok();

                    #[derive(serde::Deserialize)]
                    struct PairRequest {
                        code: String,
                    }

                    match serde_json::from_str::<PairRequest>(&body_buf) {
                        Ok(req) => {
                            let mut rl = rate_limiter.lock().unwrap();
                            if rl.try_pair(&req.code) {
                                let body = serde_json::to_string(&serde_json::json!({"paired": true})).unwrap();
                                let response = Response::from_string(body)
                                    .with_header(content_type)
                                    .with_header(cors_origin);
                                let _ = request.respond(response);
                            } else {
                                let body = serde_json::to_string(&ErrorResponse {
                                    error: "Invalid pairing code".to_string(),
                                }).unwrap();
                                let response = Response::from_string(body)
                                    .with_status_code(403)
                                    .with_header(content_type)
                                    .with_header(cors_origin);
                                let _ = request.respond(response);
                            }
                        }
                        Err(_) => {
                            let body = serde_json::to_string(&ErrorResponse {
                                error: "Invalid JSON".to_string(),
                            }).unwrap();
                            let response = Response::from_string(body)
                                .with_status_code(400)
                                .with_header(content_type)
                                .with_header(cors_origin);
                            let _ = request.respond(response);
                        }
                    }
                }
                ("GET", "/pairing-code") => {
                    // Called by the Tauri frontend to display the code
                    let rl = rate_limiter.lock().unwrap();
                    let code = rl.get_pairing_code().cloned().unwrap_or_default();
                    let locked = rl.is_locked();
                    let body = serde_json::to_string(&serde_json::json!({
                        "code": code,
                        "locked": locked
                    })).unwrap();
                    let response = Response::from_string(body)
                        .with_header(content_type)
                        .with_header(cors_origin);
                    let _ = request.respond(response);
                }
                ("GET", "/exposed-entries") => {
                    // Returns entries that were served before lockout (potentially compromised)
                    let rl = rate_limiter.lock().unwrap();
                    let exposed = rl.get_exposed().to_vec();
                    let body = serde_json::to_string(&exposed).unwrap();
                    let response = Response::from_string(body)
                        .with_header(content_type)
                        .with_header(cors_origin);
                    let _ = request.respond(response);
                }
                ("POST", "/dismiss-alert") => {
                    // User acknowledges the breach alert
                    let mut rl = rate_limiter.lock().unwrap();
                    rl.clear_exposed();
                    let body = serde_json::to_string(&serde_json::json!({"dismissed": true})).unwrap();
                    let response = Response::from_string(body)
                        .with_header(content_type)
                        .with_header(cors_origin);
                    let _ = request.respond(response);
                }
                ("GET", _) if url.starts_with("/entries") => {
                    // Rate limit check for credential access
                    {
                        let mut rl = rate_limiter.lock().unwrap();
                        if !rl.check_and_record() {
                            let code = rl.get_pairing_code().cloned().unwrap_or_default();
                            drop(rl);

                            // Notify the app that pairing is needed
                            let _ = state.pairing_code.lock().unwrap().replace(code.clone());

                            let body = serde_json::to_string(&RateLimitResponse {
                                error: "Rate limited. Too many requests too fast.".to_string(),
                                code: "RATE_LIMITED".to_string(),
                                pairing_required: true,
                            }).unwrap();
                            let response = Response::from_string(body)
                                .with_status_code(429)
                                .with_header(content_type)
                                .with_header(cors_origin);
                            let _ = request.respond(response);
                            continue;
                        }
                    }

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

                            // Track exposed entries for breach alert
                            {
                                let exposed: Vec<ExposedEntry> = entries.iter().map(|e| ExposedEntry {
                                    name: e.name.clone(),
                                    username: e.username.clone(),
                                    url: e.url.clone(),
                                }).collect();
                                let mut rl = rate_limiter.lock().unwrap();
                                rl.record_exposed(&exposed);
                            }

                            let body = serde_json::to_string(&entries).unwrap();
                            let response = Response::from_string(body)
                                .with_header(content_type)
                                .with_header(cors_origin);
                            let _ = request.respond(response);
                        }
                    }
                }
                ("POST", "/entries") => {
                    // Rate limit check for saving too
                    {
                        let mut rl = rate_limiter.lock().unwrap();
                        if !rl.check_and_record() {
                            let code = rl.get_pairing_code().cloned().unwrap_or_default();
                            drop(rl);
                            let _ = state.pairing_code.lock().unwrap().replace(code.clone());

                            let body = serde_json::to_string(&RateLimitResponse {
                                error: "Rate limited".to_string(),
                                code: "RATE_LIMITED".to_string(),
                                pairing_required: true,
                            }).unwrap();
                            let response = Response::from_string(body)
                                .with_status_code(429)
                                .with_header(content_type)
                                .with_header(cors_origin);
                            let _ = request.respond(response);
                            continue;
                        }
                    }

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

                                    // Check for duplicate
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
                                        continue;
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
                                        totp_secret: String::new(),
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
