use tauri::{State, Manager, AppHandle};
use std::sync::Mutex;
use rusqlite::{Connection, Result};
use serde::{Serialize, Deserialize};
use reqwest::Client;
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;
use chrono::Local;

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
    // Feishu Configuration
    pub feishu_app_id: Option<String>,
    pub feishu_app_secret: Option<String>,
    pub feishu_target_email: Option<String>,
    pub schedule_time: Option<String>, // Format: "HH:MM"
    pub feishu_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: "".to_string(),
            git_paths: vec![],
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            base_url: None,
            custom_rules: r#"# Role: 高效汇报助手

# Profile
- Author: Gemini
- Description: 专注于将碎片的日常工作记录转化为逻辑清晰、简明扼要的职业日报。
- Tone: 以此为基调：专业、理性、结果导向、言简意赅。

# Workflow (模式 [输入] → 变换* → 输出)
1.  **输入解析 (Input Analysis)**:
    - 接收用户输入的原始工作记录（可能包含口语、碎片化信息）。
    - 识别关键要素：完成的任务、遇到的阻碍、实施的解决方案。

2.  **核心变换 (Transformation Process)**:
    - **去噪 (Filter)**: 剔除无关的语气词和琐碎细节，保留核心动作与结果。
    - **重构 (Restructure)**:
        - 将“流水账”转换为“完成事项”（使用动宾结构，如“完成...”，“优化...”）。
        - 将“问题与解决”配对，强调闭环（即问题已解决或有明确后续）。
    - **升维 (Elevate)**: 调整措辞，使其更具职业素养（例如将“修了个bug”转换为“修复系统异常，提升稳定性”）。
    - **压缩 (Condense)**: 确保整体篇幅短小精悍，适合领导在手机端快速扫读。

3.  **输出生成 (Output Generation)**:
    - 按照预设的【日报模板】输出最终文本。

# Constraints
- 必须保持客观，避免主观情绪宣泄。
- “问题与解决方案”部分，如果问题未解决，必须给出预计解决时间或需要的支持。
- 列表项不超过 5 项，重点突前。"#.to_string(),
            report_template: r#"**【📅 日报 - MM月DD日】**

**✅ 今日产出**
* [事项1]：[结果/进度]
* [事项2]：[结果/进度]

**⚠️ 问题与对策**
* **问题**：[简述核心问题]
    **解决**：[已采取的措施 或 下一步计划]"#.to_string(),
            deep_analysis: false,
            feishu_app_id: None,
            feishu_app_secret: None,
            feishu_target_email: None,
            schedule_time: None,
            feishu_enabled: false,
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
                                 diff = Some(if raw.chars().count() > 3000 { format!("{}... (truncated)", raw.chars().take(3000).collect::<String>()) } else { raw });
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
        /// Set Feishu App ID
        #[arg(long)]
        feishu_app_id: Option<String>,
        /// Set Feishu App Secret
        #[arg(long)]
        feishu_app_secret: Option<String>,
        /// Set Feishu Target Email
        #[arg(long)]
        feishu_target: Option<String>,
        /// Set Schedule Time (HH:MM)
        #[arg(long)]
        schedule: Option<String>,
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
    /// Manage the application service (Status, Start, Stop)
    Service {
        #[command(subcommand)]
        action: ServiceCommands,
    },
    /// Internal: Run as a background daemon (do not use directly)
    Daemon,
}

#[derive(Subcommand)]
enum ServiceCommands {
    /// Check if the service is running
    Status,
    /// Start the service
    Start,
    /// Stop the service
    Stop,
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
            Commands::Config { api_key, add_repo, deep_analysis, feishu_app_id, feishu_app_secret, feishu_target, schedule } => {
                let mut config = load_config();
                let mut updated = false;
                if let Some(k) = api_key { config.api_key = k; updated = true; println!("Updated API Key"); }
                if let Some(repo) = add_repo { 
                    if !config.git_paths.contains(&repo) { config.git_paths.push(repo); updated = true; println!("Added Repo"); }
                }
                if let Some(da) = deep_analysis { config.deep_analysis = da; updated = true; println!("Updated Deep Analysis to {}", da); }
                if let Some(id) = feishu_app_id { config.feishu_app_id = Some(id); updated = true; println!("Updated Feishu App ID"); }
                if let Some(secret) = feishu_app_secret { config.feishu_app_secret = Some(secret); updated = true; println!("Updated Feishu App Secret"); }
                if let Some(target) = feishu_target { config.feishu_target_email = Some(target); updated = true; println!("Updated Feishu Target Email"); }
                if let Some(time) = schedule { 
                    config.schedule_time = Some(time); 
                    config.feishu_enabled = true; 
                    updated = true; 
                    println!("Updated Schedule Time & Enabled Feishu"); 
                }
                
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


            Commands::Service { action } => {
                match action {
                     ServiceCommands::Status => {
                         // Check daily-assistant.exe
                         let output1 = Command::new("tasklist")
                             .args(&["/FI", "IMAGENAME eq daily-assistant.exe", "/FO", "CSV", "/NH"])
                             .output()
                             .expect("Failed to execute tasklist");
                         let stdout1 = String::from_utf8_lossy(&output1.stdout);

                         // Check da.exe
                         let output2 = Command::new("tasklist")
                             .args(&["/FI", "IMAGENAME eq da.exe", "/FO", "CSV", "/NH"])
                             .output()
                             .expect("Failed to execute tasklist");
                         let stdout2 = String::from_utf8_lossy(&output2.stdout);

                         if stdout1.contains("daily-assistant.exe") || stdout2.contains("da.exe") {
                             println!("🟢 Service is RUNNING.");
                         } else {
                             println!("🔴 Service is STOPPED.");
                         }
                     },
                     ServiceCommands::Start => {
                         let exe = std::env::current_exe().unwrap();
                         
                         #[cfg(target_os = "windows")]
                         {
                             // 0x08000000 is CREATE_NO_WINDOW
                             Command::new(exe)
                                 .arg("daemon")
                                 .creation_flags(0x08000000) 
                                 .spawn()
                                 .expect("Failed to start daemon service");
                         }
                         #[cfg(not(target_os = "windows"))]
                         {
                             Command::new(exe)
                                 .arg("daemon")
                                 .spawn()
                                 .expect("Failed to start daemon service");
                         }
                         
                         println!("🚀 Service Started (Background Mode).");
                     },
                     ServiceCommands::Stop => {
                         let _ = Command::new("taskkill")
                             .args(&["/F", "/IM", "daily-assistant.exe"])
                             .output();
                         // Also kill "da.exe" just in case
                         let _ = Command::new("taskkill")
                             .args(&["/F", "/IM", "da.exe"])
                             .output();
                         println!("🛑 Service Stopped.");
                     }
                }
            },
            Commands::Daemon => {
                start_scheduler();
                // start_scheduler loops forever, so we never reach here
            }
        }
        std::process::exit(0);
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            save_log, delete_log, get_today_logs, scan_git_repos, call_ai,
            get_config, save_config
        ])
        .setup(|app| {
            let db_path = get_db_path();
            let db_state = DbState::init(db_path).expect("Failed to initialize database");
            app.manage(db_state);
            
            // Start Scheduler
            // Start Scheduler (Thread) - Only if running GUI mode
            std::thread::spawn(move || {
                start_scheduler();
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Feishu Client
struct FeishuClient {
    app_id: String,
    app_secret: String,
}

impl FeishuClient {
    fn new(app_id: String, app_secret: String) -> Self {
        Self { app_id, app_secret }
    }

    async fn get_token(&self) -> Result<String, String> {
        let client = Client::new();
        let res = client.post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&serde_json::json!({
                "app_id": self.app_id,
                "app_secret": self.app_secret
            }))
            .send()
            .await.map_err(|e| e.to_string())?;
            
        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        if let Some(token) = json.get("tenant_access_token") {
            Ok(token.as_str().unwrap().to_string())
        } else {
            Err(format!("Auth Failed: {:?}", json))
        }
    }

    async fn get_user_id(&self, token: &str, email: &str) -> Result<String, String> {
        let client = Client::new();
        let url = "https://open.feishu.cn/open-apis/contact/v3/users/batch_get_id?user_id_type=open_id";
        let res = client.post(url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&serde_json::json!({
                "emails": [email]
            }))
            .send()
            .await.map_err(|e| e.to_string())?;
            
        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        // Path: data.user_list[0].user_id
        if let Some(list) = json.get("data").and_then(|d| d.get("user_list")).and_then(|l| l.as_array()) {
            if let Some(user) = list.first() {
                if let Some(id) = user.get("user_id") {
                    return Ok(id.as_str().unwrap().to_string());
                }
            }
        }
        Err(format!("User not found for email: {}", email))
    }

    async fn send_message(&self, token: &str, receive_id: &str, content: &str) -> Result<(), String> {
        let client = Client::new();
        let url = "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type=open_id";
        let res = client.post(url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&serde_json::json!({
                "receive_id": receive_id,
                "msg_type": "text",
                "content": serde_json::json!({ "text": content }).to_string()
            }))
            .send()
            .await.map_err(|e| e.to_string())?;
            
        let status = res.status();
        if !status.is_success() {
             let text = res.text().await.unwrap_or_default();
             return Err(format!("Send failed: {} - {}", status, text));
        }
        Ok(())
    }
}

// Scheduler Logic
fn start_scheduler() {
    let rt = Runtime::new().unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        let config = load_config();
        
        if !config.feishu_enabled { continue; }
        if let Some(time_str) = config.schedule_time.clone() {
            let now = Local::now().format("%H:%M").to_string();
            if now == time_str {
                println!("⏰ It's time! ({}) Starting scheduled report...", now);
                // Trigger logic
                rt.block_on(async {
                    if let Err(e) = run_scheduled_job(config).await {
                        println!("❌ Scheduled Job Failed: {}", e);
                    }
                });
                // Avoid double-running in the same minute
                std::thread::sleep(std::time::Duration::from_secs(60)); 
            }
        }
    }
}

async fn run_scheduled_job(config: AppConfig) -> Result<(), String> {
    // 1. Collect Data
    // We need DB access. Since we are in a thread, we can try to use app.state().
    // But rusqlite usage in threads is tricky if not careful.
    // For simplicity, we might just open a new connection or use the CLI commands' logic.
    // Let's re-use the logic from Review command but purely in Rust.
    
    let db_path = get_db_path();
    let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
    
    // 1. Logs
    let mut logs = vec![];
    let mut stmt = conn.prepare("SELECT id, content, log_type, timestamp FROM logs WHERE date(timestamp) = date('now', 'localtime') ORDER BY id DESC").map_err(|e| e.to_string())?;
    let iter = stmt.query_map([], |row| Ok(LogItem { id: row.get(0)?, content: row.get(1)?, log_type: row.get(2)?, timestamp: row.get(3)? })).map_err(|e| e.to_string())?;
    for l in iter { logs.push(l.unwrap()); }

    // 2. Commits
    let commits = scan_git_repos(config.git_paths.clone(), config.deep_analysis).unwrap_or_default();

    if logs.is_empty() && commits.is_empty() {
        return Err("No logs or commits today. Skipping report.".to_string());
    }

    // 3. Prompt
    let prompt = generate_prompt_logic(&logs, &commits, &config, "analysis");

    // 4. AI
    let req = AiRequest {
        provider: config.provider.clone(),
        api_key: config.api_key.clone(),
        model: config.model.clone(),
        base_url: config.base_url.clone(),
        prompt,
    };
    let report = call_ai(req).await?;

    // 5. Send to Feishu
    if let (Some(app_id), Some(secret), Some(target)) = (config.feishu_app_id, config.feishu_app_secret, config.feishu_target_email) {
        println!("🚀 Sending to Feishu...");
        let client = FeishuClient::new(app_id, secret);
        let token = client.get_token().await?;
        let user_id = client.get_user_id(&token, &target).await?;
        client.send_message(&token, &user_id, &report).await?;
        println!("✅ Feishu Message Sent!");
    } else {
        println!("⚠️ Feishu config missing, skipping send.");
    }
    
    Ok(())
}
