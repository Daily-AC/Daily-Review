use tauri::{State, Manager};
use std::sync::Mutex;
use rusqlite::{Connection, Result};
use serde::{Serialize, Deserialize};
use reqwest::Client;
use std::process::Command;

// Domain Models
#[derive(Debug, Serialize, Deserialize)]
pub struct LogItem {
    id: Option<i64>,
    content: String,
    log_type: String, // "task", "note", "problem"
    timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitCommit {
    hash: String,
    message: String,
    author: String,
    time: i64,
    repo_name: Option<String>, // Added to distinguish repos
}

// Database Service
pub struct DbState {
    conn: Mutex<Connection>,
}

impl DbState {
    fn init() -> Result<Self> {
        let conn = Connection::open("daily_assistant.db")?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY,
                content TEXT NOT NULL,
                log_type TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        
        Ok(DbState { conn: Mutex::new(conn) })
    }
}

#[derive(Deserialize)]
struct AiRequest {
    provider: String,
    api_key: String,
    model: String,
    prompt: String,
    base_url: Option<String>,
}

// Commands
#[tauri::command]
fn save_log(state: State<DbState>, content: String, log_type: String) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|_| "Failed to lock db".to_string())?;
    
    conn.execute(
        "INSERT INTO logs (content, log_type) VALUES (?1, ?2)",
        [&content, &log_type],
    ).map_err(|e| e.to_string())?;
    
    Ok("Log saved successfully".to_string())
}

#[tauri::command]
fn get_today_logs(state: State<DbState>) -> Result<Vec<LogItem>, String> {
    let conn = state.conn.lock().map_err(|_| "Failed to lock db".to_string())?;
    
    let mut stmt = conn.prepare("SELECT id, content, log_type, timestamp FROM logs ORDER BY id DESC").map_err(|e| e.to_string())?;
    let logs_iter = stmt.query_map([], |row| {
        Ok(LogItem {
            id: row.get(0)?,
            content: row.get(1)?,
            log_type: row.get(2)?,
            timestamp: row.get(3)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut logs = Vec::new();
    for log in logs_iter {
        logs.push(log.map_err(|e| e.to_string())?);
    }
    
    Ok(logs)
}

#[tauri::command]
fn scan_git_repos(paths: Vec<String>) -> Result<Vec<GitCommit>, String> {
    let mut all_commits = Vec::new();

    for path in paths {
        let repo_name = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        #[cfg(target_os = "windows")]
        let output = Command::new("git")
            .args(&["-C", &path, "log", "--since=midnight", "--pretty=format:%H|%s|%an|%at"])
            .output();

        #[cfg(not(target_os = "windows"))]
        let output = Command::new("git")
            .args(&["-C", &path, "log", "--since=midnight", "--pretty=format:%H|%s|%an|%at"])
            .output();

        // Skip failures for individual repos, just log them?
        // For now, if one fails, we just continue
        if let Ok(out) = output {
             if out.status.success() {
                 let stdout = String::from_utf8_lossy(&out.stdout);
                 for line in stdout.lines() {
                     let parts: Vec<&str> = line.split('|').collect();
                     if parts.len() >= 4 {
                         all_commits.push(GitCommit {
                             hash: parts[0].to_string(),
                             message: parts[1].to_string(),
                             author: parts[2].to_string(),
                             time: parts[3].parse().unwrap_or(0),
                             repo_name: Some(repo_name.clone()),
                         });
                     }
                 }
             }
        }
    }

    Ok(all_commits)
}

#[tauri::command]
async fn call_ai(request: AiRequest) -> Result<String, String> {
    let client = Client::new();
    
    match request.provider.as_str() {
        "openai" => {
            let url = request.base_url.unwrap_or("https://api.openai.com/v1".to_string()) + "/chat/completions";
            let body = serde_json::json!({
                "model": request.model,
                "messages": [{"role": "user", "content": request.prompt}],
                "temperature": 0.7
            });
            
            let res = client.post(&url)
                .header("Authorization", format!("Bearer {}", request.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await.map_err(|e| e.to_string())?;
                
            let text = res.text().await.map_err(|e| e.to_string())?;
            let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            
            if let Some(err) = json.get("error") {
                return Err(err.to_string());
            }
            Ok(json["choices"][0]["message"]["content"].as_str().unwrap_or(&text).to_string())
        },
        _ => Err("Provider Not Supported Yet".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_state = DbState::init().expect("Failed to initialize database");

    tauri::Builder::default()
        .manage(db_state)
        .invoke_handler(tauri::generate_handler![
            save_log, 
            get_today_logs,
            scan_git_repos,
            call_ai
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
