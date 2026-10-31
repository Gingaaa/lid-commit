# 🔒 LidCommit

**Autonomous Git auto-commit and push powered by AI when you close your laptop lid.**

Never lose unsaved progress again. LidCommit runs quietly on your system, automatically detects your actively open IDE workspaces, drafts intelligent commit messages from your git diffs using Gemini/OpenAI/Ollama, commits your changes, and pushes them to your remote repository.

When you open another device, simply run `git pull` and continue right where you left off.

---

## ⚡ Crucial Setup: Windows Lid Close Power Setting

> [!IMPORTANT]
> To allow LidCommit to safely inspect your repositories, stage changes, generate AI messages, and push to GitHub while the lid is down, Windows must not abruptly put the network card and CPU to sleep before the git operations complete.

### How to configure "Do Nothing" on Lid Close:
1. Press `Win + R`, type **`control`**, and hit **Enter** to open the classic **Control Panel**.
2. Set *View by* (top right) to **Large icons** or **Small icons**, then click **Power Options**.
3. In the left sidebar, click **"Choose what closing the lid does"**.
4. Locate the row **"When I close the lid"**:
   - Set **On battery** to **Do nothing**.
   - Set **Plugged in** to **Do nothing**.
5. Click **Save changes**.

Your screen will now turn off safely when closing the lid, but your background processes, Wi-Fi network connection, and LidCommit Git sync will execute seamlessly without being abruptly cut off!

---

## ✨ Countribution


## ✨ Features

- 🖥️ **Native Desktop Dashboard & System Tray** — Modern dark-mode GUI to manage projects, monitor live daemon status, and review commit logs.
- ⏸️ **Per-Project Pause / Resume Controls** — Choose which specific repositories should auto-commit on lid close and which ones to skip.
- ⚡ **Instant "Sync Now" & "Commit This"** — Trigger manual commits across all or individual repositories anytime without needing to close your lid.
- 🎯 **Active Window Filter** — Only tracks projects currently open in an active IDE window; closed historical folders are automatically ignored.
- 🔒 **Native Lid Close Detection** — Real-time OS power broadcast monitoring with zero CPU polling.
- 🤖 **AI & Fallback Commit Messages** — Context-aware commit messages generated from your diffs via Gemini/OpenAI/Ollama, with seamless automatic fallback to `[LidCommit] auto-save at <timestamp>` when no API key is provided or offline.
- 🔍 **IDE-Aware** — Detects VS Code, Antigravity IDE, Cursor, Windsurf, JetBrains, and any running process with a git root.
- 🚀 **Auto Push** — Automatically pushes to your branch with safety checks.
- 🛡️ **Safety Guards** — Automatically skips if there are merge conflicts, untracked bulk files (>50), missing remotes, or if the repo is paused.
- 🔔 **OS Notifications & Popups** — Foreground status summary modal upon wake-up so you always know what was synced.
- ⚙️ **TOML Config & In-App Preferences** — Edit preferences directly in the desktop interface or via `config.toml`.
- 🌍 **Cross-Platform** — Windows, macOS, and Linux.

---

## 🖥️ Desktop Dashboard Controls

When you launch `lid-commit.exe`, it docks in your system tray and displays the native dashboard:

| Feature | Description |
|---|---|
| **📁 Detected Projects** | Real-time list of all git repositories open in your active IDE windows with diff indicators (*Clean* vs *Uncommitted Changes*). |
| **⏸️ Pause Auto-Commit** | Toggle button on each repository card to selectively exempt projects from lid-close commits. |
| **⚡ Commit This** | Manually commits and pushes a single project on demand. |
| **⚡ Sync Now** | Forces an immediate safe commit & push across all unpaused projects. |
| **⏸️ Pause / ▶️ Resume Daemon** | Global pause toggle to temporarily suspend all background lid monitoring. |
| **📜 Activity & History** | Live feed of recent automated commits with timestamp and file count. |
| **⚙️ Configuration** | Configure AI providers (Gemini / OpenAI / Ollama), push settings, and safety thresholds. |

## 📦 Easy Download & Installation

LidCommit installs just like any standard modern application — **zero technical setup, compiling, or Rust installations required for end users!**

### 🪟 Windows (Recommended: 1-Click Setup Installer)

1. **Download** the latest **[`LidCommit-Setup-x86_64.exe`](https://github.com/Gingaaa/lid-commit/releases/latest)** installer from GitHub Releases.
2. Double-click the `.exe` and follow the setup wizard:
   - Installs cleanly into your Windows Apps directory (`%LocalAppData%\Programs\LidCommit`).
   - Automatically registers **LidCommit** in your **Windows Start Menu** and creates an optional **Desktop Shortcut**.
   - Optional: Check *"Automatically launch LidCommit on Windows startup"* so your work is always protected.
   - Clean, standard uninstaller registered in Windows *Installed apps / Add or remove programs*.
3. Launch **LidCommit** from your Start menu or system tray!

> *Alternatively, portable standalone zip packages (`lid-commit-windows-x86_64.zip`) are also available under [GitHub Releases](https://github.com/Gingaaa/lid-commit/releases).*

---

### 🍎 macOS (Pre-Built Binary)

1. Download `lid-commit-macos-aarch64.tar.gz` (Apple Silicon M1/M2/M3) or `lid-commit-macos-x86_64.tar.gz` (Intel) from [Releases](https://github.com/Gingaaa/lid-commit/releases/latest).
2. Extract and move `lid-commit` into `/usr/local/bin` or your `Applications` folder:
   ```bash
   tar -xvf lid-commit-macos-*.tar.gz
   sudo mv lid-commit /usr/local/bin/
   ```
3. Run `lid-commit` from terminal, or add it to macOS *Login Items* for automatic background protection.

---

### 🐧 Linux (Pre-Built Binary)

1. Download `lid-commit-linux-x86_64.tar.gz` from [Releases](https://github.com/Gingaaa/lid-commit/releases/latest).
2. Extract and install:
   ```bash
   tar -xvf lid-commit-linux-x86_64.tar.gz
   sudo mv lid-commit /usr/local/bin/
   ```
3. (Optional) Run as a `systemd` user service to keep it running smoothly in the background.

---

### 🛠️ Build from Source (For Developers & Contributors)

If you are developing or contributing to LidCommit:

```bash
# Ensure Rust toolchain (v1.75+) is installed
git clone https://github.com/Gingaaa/lid-commit.git
cd lid-commit
cargo build --release

# Run GUI dashboard:
./target/release/lid-commit

# To build the Windows Inno Setup installer:
iscc installer.iss
```

### 3. Configure Your AI Key

Set your environment variable or configure it via the dashboard settings tab:

**Windows (PowerShell):**
```powershell
[System.Environment]::SetEnvironmentVariable("GEMINI_API_KEY", "your_gemini_key_here", "User")
```

**macOS / Linux:**
```bash
export GEMINI_API_KEY="your_gemini_key_here"
```

---

## 🤖 AI Providers & Fallback Commit Messages

| Provider | Model Examples | Setup | In-App UI Configuration |
|---|---|---|---|
| **Google Gemini** (default) | `gemini-2.0-flash`, `gemini-1.5-pro` | Free key from [Google AI Studio](https://aistudio.google.com/) | Enter API key directly in Dashboard UI or use `GEMINI_API_KEY` |
| **Ollama** (offline) | `llama3.2`, `codellama`, `mistral` | Runs 100% offline via local Ollama instance | Configure custom local URL (default: `http://localhost:11434`) & model |
| **OpenAI** | `gpt-4o-mini`, `gpt-4o` | Set `OPENAI_API_KEY` | Enter API key directly in Dashboard UI or use `OPENAI_API_KEY` |
| **Clean Auto-Save** | Timestamps (offline) | Zero configuration / no API keys required | 1-click toggle in Dashboard UI |

You can switch between any of the 4 providers directly in the **Desktop Dashboard Settings tab** with one click. When one provider is selected, the others are deactivated, and the relevant configuration fields (API key input, endpoint URL, or model tag) adapt dynamically.

### 📝 Fallback Commit Message (No API Key Required)

If you don't have a Google Gemini API key or any other AI API key configured, or if your machine is offline when the laptop lid closes, **LidCommit still works seamlessly without any errors**. 

In such cases, it automatically switches to a clean, standardized timestamp-based commit message:

```text
[LidCommit] auto-save at YYYY-MM-DD HH:MM
```

*(Example: `[LidCommit] auto-save at 2026-10-31 11:25`)*

- You can customize the commit prefix (default: `[LidCommit]`) anytime via `config.toml` or the dashboard settings.
- When an API key is provided later, LidCommit will automatically resume generating descriptive, AI-powered summaries of your diffs.

---

## 🛡️ Safety Guards

LidCommit **skips** auto-committing a project if:
- ⏸️ Project is paused by the user in the dashboard.
- 🧹 Working tree is clean (no modified or untracked files).
- 📦 More than 50 files changed (guards against unignored `node_modules`, `target/`, or build artifacts).
- 🌐 No remote repository configured (prevents dangling commits).
- 💥 Merge conflicts exist in the working tree.
- 🚫 Path matches user-defined `excluded_paths` in `config.toml`.

---

## 🏗️ Architecture

```
src/
├── main.rs          — Daemon entry point, event loop, manual sync dispatch
├── gui.rs           — Native desktop window, system tray, and IPC controller
├── dashboard.html   — Modern dark-mode dashboard UI
├── config.rs        — Configuration parser & serializer
├── lid_monitor/     — Native OS lid-switch & sleep event listeners
│     Windows: WM_POWERBROADCAST + GUID_LIDSWITCH_STATE_CHANGE
│     macOS:   IOKit kIOMessageSystemWillSleep
│     Linux:   logind D-Bus PrepareForSleep
├── ide_scanner/     — Active IDE window & process scanner
├── git/             — Safe git diff analysis, add, commit, and push
├── ai/              — AI commit message generator (Gemini / OpenAI / Ollama)
└── notifier.rs      — Toast notifications and resume modal dialogs
```

---

## 📄 License

MIT © [Anshu Kumar](https://github.com/Gingaaa)

