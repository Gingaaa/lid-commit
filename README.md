# 🔒 LidCommit

**Auto-commit your code changes when you close your laptop lid.**

Note the Windows setting requirement: "Set 'When I close the lid' to 'Do nothing' in Control Panel > Power Options so background Git sync runs uninterrupted."

Never lose progress again. LidCommit runs silently in the background and, when you close your laptop, automatically detects your open IDE projects, generates an AI-powered commit message using Gemini, and commits + pushes your changes to GitHub.

When you pick up another device, just `git pull` and carry on exactly where you left off.

---

## ✨ Features

- 🔒 **Lid close detection** — native OS events (no polling)
- 🤖 **AI commit messages** — Gemini generates meaningful messages from your actual diff
- 🔍 **IDE-aware** — detects VS Code, JetBrains IDEs, and any git repo in a running process's CWD
- 🚀 **Auto push** — commits and pushes to your branch automatically
- 🛡️ **Safety guards** — skips on merge conflicts, too many files, or no remote
- 🔔 **OS notifications** — toast notification tells you what was committed
- ⚙️ **TOML config** — fully configurable per machine
- 🌍 **Cross-platform** — Windows, macOS, Linux

---

## 🚀 Quick Start

### 1. Prerequisites

- [Rust](https://rustup.rs/) installed
- `git` in your PATH
- A [Gemini API key](https://aistudio.google.com/app/apikey) (free)

### 2. Build

```bash
git clone https://github.com/yourusername/lid-commit
cd lid-commit
cargo build --release
```

### 3. Set your API key

```bash
# Linux / macOS
export GEMINI_API_KEY=your_key_here

# Windows (PowerShell)
[System.Environment]::SetEnvironmentVariable("GEMINI_API_KEY", "your_key_here", "User")
```

### 4. Install as a background service

**Windows (run PowerShell as Administrator):**
```powershell
.\install\windows\install.ps1 -ApiKey "your_gemini_key"
```

**macOS:**
```bash
./install/macos/install.sh
```

**Linux:**
```bash
cargo build --release
cp target/release/lid-commit ~/.local/bin/
cp install/linux/lidcommit.service ~/.config/systemd/user/
echo "GEMINI_API_KEY=your_key" > ~/.config/lid-commit/.env
systemctl --user enable --now lidcommit
```

---

## 🤖 AI Providers

| Provider | Model Examples | Requires |
|---|---|---|
| **Gemini** (default) | `gemini-2.0-flash`, `gemini-1.5-pro` | `GEMINI_API_KEY` |
| **OpenAI** | `gpt-4o-mini`, `gpt-4o` | `OPENAI_API_KEY` |
| **Ollama** (offline) | `llama3.2`, `codellama` | Ollama running locally |

### Example AI-generated commit messages:

```
🔒 lid-close: add JWT middleware and fix session expiry handling
🔒 lid-close: refactor database connection pooling for performance
🔒 lid-close: fix null pointer in user authentication flow
```

---

## 🛡️ Safety Guards

LidCommit **skips** a project if:
- Working tree is clean (nothing to commit)
- More than 50 files changed (likely forgot `.gitignore`)
- No remote configured (nowhere to push)
- Merge conflicts present (would corrupt commit)
- Path is in user-defined `excluded_paths`

---

## 📊 Flow

```
Lid closes → detect open IDEs → for each project:
  git status → check safety → build diff →
  Gemini API → commit message → git add → git commit → git push
  → OS notification
```

---

## 🏗️ Architecture

```
src/
├── main.rs          — Daemon entry point, event loop
├── config.rs        — TOML config loading
├── lid_monitor/mod.rs  — Platform lid close detection
│     Windows: WM_POWERBROADCAST + GUID_LIDCLOSE_ACTION
│     macOS:   IOKit kIOMessageSystemWillSleep
│     Linux:   logind D-Bus PrepareForSleep
├── ide_scanner/mod.rs  — Find open IDE projects
├── git/mod.rs          — git status, diff, add, commit, push
├── ai/mod.rs           — Gemini / OpenAI / Ollama
└── notifier.rs         — OS toast notifications
```

---

## 📄 License

MIT
