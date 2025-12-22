use tauri::{State, Manager};
use std::sync::Mutex;
use rusqlite::{Connection, Result};
use serde::{Serialize, Deserialize};
use reqwest::Client;
use std::process::Command;
use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;

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
    repo_name: Option<String>,
    diff: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub api_key: String,
    pub git_paths: Vec<String>,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub custom_rules: String,
    pub report_template: String,
    pub deep_analysis: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: "".to_string(),
            git_paths: vec![],
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            base_url: None,
            custom_rules: "".to_string(),
            report_template: "Example:\n1. Progress: ...\n2. Problems: ...\n3. Plan for tomorrow: ...".to_string(),
            deep_analysis: false,
        }
    }
}

// Database Service
pub struct DbState {
    conn: Mutex<Connection>,
}

impl DbState {
    fn init(path: std::path::PathBuf) -> Result<Self> {
        let conn = Connection::open(path)?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY,
                content TEXT NOT NULL,
                log_type TEXT NOT NULL,
                timestamp DATETIME DEFAULT (datetime('now', 'localtime'))
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

// Configuration Helpers
fn get_config_path() -> std::path::PathBuf {
    let identifier = "com.tauri-app.daily-assistant";
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var("APPDATA").expect("APPDATA not set");
        let path = std::path::PathBuf::from(app_data).join(identifier);
        std::fs::create_dir_all(&path).expect("failed to create app data dir");
        path.join("config.json")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").expect("HOME not set");
        let path = std::path::PathBuf::from(home).join(".config").join(identifier);
        std::fs::create_dir_all(&path).expect("failed to create app data dir");
        path.join("config.json")
    }
}

fn load_config() -> AppConfig {
    let path = get_config_path();
    if path.exists() {
        let content = std::fs::read_to_string(path).unwrap_or("{}".to_string());
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        AppConfig::default()
    }
}

fn save_config_file(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(())
}

// Prompt Generation Logic (Ported from Frontend)
fn generate_prompt_logic(logs: &[LogItem], commits: &[GitCommit], config: &AppConfig, mode: &str) -> String {
    let logs_text = logs.iter().map(|l| format!("- {}", l.content)).collect::<Vec<_>>().join("\n");
    let git_text = commits.iter().map(|g| {
        let mut text = format!("- [{}] {}", g.repo_name.as_deref().unwrap_or("?"), g.message);
        if let Some(diff) = &g.diff {
             text.push_str(&format!("\n  Code Diff Summary:\n```\n{}\n```", diff));
        }
        text
    }).collect::<Vec<_>>().join("\n");

    let base_instruction = if mode == "analysis" {
        "Provide a comprehensive summary, 3 improvements, and 1 key knowledge point. If code diffs are provided, use them to explain technical details.".to_string()
    } else { // export
        format!("Strictly follow the format below:\n\nFormat Template:\n{}", config.report_template)
    };

    format!(r#"
        Context:
        Manual Logs:
        {}
        
        Git Commits:
        {}
        
        System Instruction:
        {}
        
        Additional User Rules:
        {}
    "#, logs_text, git_text, base_instruction, config.custom_rules)
}


// Tauri Commands
#[tauri::command]
fn get_config() -> AppConfig {
    load_config()
}

#[tauri::command]
fn save_config(config: AppConfig) -> Result<(), String> {
    save_config_file(&config)
}


#[tauri::command]
fn save_log(state: State<DbState>, content: String, log_type: String) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|_| "Failed to lock db".to_string())?;
    conn.execute(
        "INSERT INTO logs (content, log_type, timestamp) VALUES (?1, ?2, datetime('now', 'localtime'))",
        [&content, &log_type],
    ).map_err(|e| e.to_string())?;
    Ok("Log saved successfully".to_string())
}

#[tauri::command]
fn delete_log(state: State<DbState>, id: i64) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|_| "Failed to lock db".to_string())?;
    conn.execute("DELETE FROM logs WHERE id = ?1", [&id]).map_err(|e| e.to_string())?;
    Ok("Log deleted successfully".to_string())
}

#[tauri::command]
fn get_today_logs(state: State<DbState>) -> Result<Vec<LogItem>, String> {
    let conn = state.conn.lock().map_err(|_| "Failed to lock db".to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, content, log_type, timestamp FROM logs 
         WHERE date(timestamp) = date('now', 'localtime')
         ORDER BY id DESC"
    ).map_err(|e| e.to_string())?;

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
fn scan_git_repos(paths: Vec<String>, deep_analysis: bool) -> Result<Vec<GitCommit>, String> {
    let mut all_commits = Vec::new();

    for path in paths {
        let repo_name = std::path::Path::new(&path).file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string();
        
        let args = &["-C", &path, "log", "--since=midnight", "--pretty=format:%H|%s|%an|%at"];
        #[cfg(target_os = "windows")]
        let output = Command::new("git").args(args).output();
        #[cfg(not(target_os = "windows"))]
        let output = Command::new("git").args(args).output();

        if let Ok(out) = output {
             if out.status.success() {
                 let stdout = String::from_utf8_lossy(&out.stdout);
                 for line in stdout.lines() {
                     let parts: Vec<&str> = line.split('|').collect();
                     if parts.len() >= 4 {
                         let hash = parts[0].to_string();
                         let message = parts[1].to_string();
                         let mut diff = None;
                         if deep_analysis {
                             let diff_args = &["-C", &path, "show", &hash, "--pretty=", "--patch", "--max-count=1"];
                             #[cfg(target_os = "windows")]
                             let dout = Command::new("git").args(diff_args).output();
                             #[cfg(not(target_os = "windows"))]
                             let dout = Command::new("git").args(diff_args).output();
                             
                             if let Ok(d) = dout {
                                 let raw = String::from_utf8_lossy(&d.stdout).to_string();
                                 diff = Some(if raw.len() > 3000 { format!("{}... (truncated)", &raw[..3000]) } else { raw });
                             }
                         }
                         all_commits.push(GitCommit {
                             hash, message, author: parts[2].to_string(), time: parts[3].parse().unwrap_or(0),
                             repo_name: Some(repo_name.clone()), diff,
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
    if let Some(err) = json.get("error") { return Err(err.to_string()); }
    Ok(json["choices"][0]["message"]["content"].as_str().unwrap_or(&text).to_string())
}



#[derive(Parser)]
#[command(name = "Daily Assistant", version, about = "A Daily Review Helper")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new log entry
    Add { 
        /// The content of the log/note
        content: String 
    },
    /// List today's logs
    List,
    /// Delete a log by ID
    Del { 
        /// The ID of the log to delete
        id: i64 
    },
    /// Configure settings (Set --api-key, --add-repo, --deep-analysis)
    Config {
        /// Set your OpenAI (or compatible) API Key
        #[arg(long)]
        api_key: Option<String>,
        /// Add a new Git repository path
        #[arg(long)]
        add_repo: Option<String>,
        /// Enable or disable Deep Git Analysis (fetching code diffs)
        #[arg(long)]
        deep_analysis: Option<bool>,
    },
    /// Sync Git repositories (Use --deep to include diffs)
    Sync {
        /// Force a deep sync (fetch code diffs) regardless of config
        #[arg(long)]
        deep: bool,
    },
    /// Generate AI Review (Default) or Export Report (Use --export)
    Review {
        /// Generate a formal report based on template instead of analysis
        #[arg(long)]
        export: bool,
    },
}

fn get_db_path() -> std::path::PathBuf {
    let identifier = "com.tauri-app.daily-assistant";
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var("APPDATA").expect("APPDATA not set");
        let path = std::path::PathBuf::from(app_data).join(identifier);
        std::fs::create_dir_all(&path).expect("failed to create app data dir");
        path.join("daily_assistant.db")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").expect("HOME not set");
        let path = std::path::PathBuf::from(home).join(".config").join(identifier);
        std::fs::create_dir_all(&path).expect("failed to create app data dir");
        path.join("daily_assistant.db")
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cli = Cli::parse();

    if let Some(cmd) = cli.command {
        // HEADLESS CLI EXECUTION
        let db_path = get_db_path();
        let db_state = DbState::init(db_path).expect("Failed to initialize database");
        let conn = db_state.conn.lock().unwrap();

        match cmd {
            Commands::Add { content } => {
                conn.execute("INSERT INTO logs (content, log_type, timestamp) VALUES (?1, ?2, datetime('now', 'localtime'))", [&content, &"note".to_string()]).unwrap();
                println!("✅ Note added: {}", content);
            },
            Commands::List => {
                let mut stmt = conn.prepare("SELECT id, timestamp, content FROM logs WHERE date(timestamp) = date('now', 'localtime') ORDER BY id ASC").unwrap();
                let logs = stmt.query_map([], |row| Ok((row.get::<_,i64>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?))).unwrap();
                println!("📅 Today's Notes:");
                for log in logs { if let Ok((id, ts, content)) = log { println!("[{}] {}  {}", id, &ts.split_whitespace().nth(1).unwrap_or(&ts)[0..5], content); } }
            },
            Commands::Del { id } => {
                 if conn.execute("DELETE FROM logs WHERE id = ?1", [&id]).unwrap() > 0 { println!("🗑️ Deleted note ID: {}", id); } 
                 else { println!("❌ Note ID {} not found.", id); }
            },
            Commands::Config { api_key, add_repo, deep_analysis } => {
                let mut config = load_config();
                let mut updated = false;
                if let Some(k) = api_key { config.api_key = k; updated = true; println!("Updated API Key"); }
                if let Some(repo) = add_repo { 
                    if !config.git_paths.contains(&repo) { config.git_paths.push(repo); updated = true; println!("Added Repo"); }
                }
                if let Some(da) = deep_analysis { config.deep_analysis = da; updated = true; println!("Updated Deep Analysis to {}", da); }
                
                if updated { save_config_file(&config).unwrap(); }
                println!("Current Config: {:#?}", config);
            },
            Commands::Sync { deep } => {
                let config = load_config();
                let use_deep = deep || config.deep_analysis;
                println!("🔄 Syncing Git Repos (Deep Analysis: {})...", use_deep);
                
                match scan_git_repos(config.git_paths.clone(), use_deep) {
                    Ok(commits) => {
                         for c in commits {
                             println!("[{}] {} ({})", c.repo_name.unwrap_or_default(), c.message, c.author);
                             if let Some(diff) = c.diff { println!("   Diff: {} bytes", diff.len()); }
                         }
                    },
                    Err(e) => println!("❌ Sync Failed: {}", e),
                }
            },
            Commands::Review { export } => {
                let config = load_config();
                let rt = tokio::runtime::Runtime::new().unwrap();
                
                // 1. Logs
                let mut logs = vec![];
                let mut stmt = conn.prepare("SELECT id, content, log_type, timestamp FROM logs WHERE date(timestamp) = date('now', 'localtime') ORDER BY id DESC").unwrap();
                let iter = stmt.query_map([], |row| Ok(LogItem { id: row.get(0)?, content: row.get(1)?, log_type: row.get(2)?, timestamp: row.get(3)? })).unwrap();
                for l in iter { logs.push(l.unwrap()); }

                // 2. Commits
                let use_deep = config.deep_analysis;
                let commits = scan_git_repos(config.git_paths.clone(), use_deep).unwrap_or_default();

                // 3. Generate Prompt
                let mode = if export { "export" } else { "analysis" };
                println!("🤔 Generating AI {}...", if export { "Report" } else { "Review" });
                let prompt = generate_prompt_logic(&logs, &commits, &config, mode);

                // 4. Call AI
                let req = AiRequest {
                    provider: config.provider.clone(),
                    api_key: config.api_key.clone(),
                    model: config.model.clone(),
                    base_url: config.base_url.clone(),
                    prompt,
                };

                match rt.block_on(call_ai(req)) {
                    Ok(res) => println!("\n{}", res),
                    Err(e) => println!("❌ AI Error: {}", e),
                }
            }
        }
        std::process::exit(0);
    }

    tauri::Builder::default()
        .setup(|app| {
            let db_path = get_db_path();
            let db_state = DbState::init(db_path).expect("Failed to initialize database");
            app.manage(db_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            save_log, delete_log, get_today_logs, scan_git_repos, call_ai,
            get_config, save_config // Exposed config commands
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
