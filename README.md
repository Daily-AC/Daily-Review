# Daily Assistant (日报助手)

A powerful, cross-platform daily review assistant built with **Tauri v2** + **React** + **Rust**.

It helps developers track their daily work, sync git commits, and use AI to generate daily reports automatically.

## ✨ Features

- **📝 Daily Logs**: Fast and persistent local logging (SQLite).
- **🔄 Git Sync**: Automatically fetch commits from multiple local repositories.
- **🧠 AI Review**:
  - **Analysis**: Summarize daily work, suggest improvements, and extract knowledge points.
  - **Deep Analysis**: Fetch code diffs for a deeper technical review.
  - **Export**: Generate formal daily reports based on custom templates.
- **💻 Full CLI Support**: `da` command-line tool for all features.
- **🎨 Modern GUI**: A beautiful, dark-themed interface built with React.

## 🚀 Installation & Setup

### 1. Install via MSI

Download and run the latest installer from the `release` folder:
`daily-assistant_0.1.0_x64-setup.exe`

### 2. Enable CLI (`da` command)

After installation, to use the `da` command globally:

1. Go to the installation folder (e.g., `%LOCALAPPDATA%\Programs\daily-assistant`).
2. Run `resources/setup-env.ps1` (Right-click -> "Run with PowerShell").
3. Restart your terminal.

## 🛠 Usage

### CLI (`da`)

The CLI shares the same configuration as the GUI. Changes made in one are reflected in the other.

```powershell
# 1. Add a log
da add "Fixed login bug"

# 2. List today's logs
da list

# 3. Config Settings
da config --api-key "sk-..."
da config --add-repo "C:\Work\Project"
da config --deep-analysis true  # Enable code diff analysis

# 4. Sync Git Activity
da sync
da sync --deep  # Force deep sync

# 5. Generate Report
da review           # AI Analysis
da review --export  # Generate Report based on template
da help             # Show help message
```

### GUI

- Launch `Daily Assistant` from start menu.
- **Today Tab**: Manage logs and sync git.
- **Review Tab**: One-click AI analysis or Export.
- **Settings Tab**: Configure API Key, Prompts, and Templates.

## 🏗 Development

### Prerequisites

- Node.js & npm
- Rust (latest stable) & Cargo

### Commands

```bash
# Install dependencies
npm install

# Run in Dev Mode (GUI + Console)
npm run tauri dev

# Build Release (Windows)
npm run tauri build
```

The build output will be in `src-tauri/target/release/`.

## 📂 Configuration Storage

Config is stored in `%APPDATA%\com.tauri-app.daily-assistant\config.json`.
Database is stored in `%APPDATA%\com.tauri-app.daily-assistant\daily_assistant.db`.
