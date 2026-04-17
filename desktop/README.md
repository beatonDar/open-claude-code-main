# Open Claude Code Desktop

Desktop GUI wrapper for Open Claude Code with Ollama support.

## Prerequisites

- [Bun](https://bun.sh) installed
- [Ollama](https://ollama.com) installed and running
- [Rust](https://rustup.rs) for building Tauri

## Quick Start

### Option 1: Use CLI Only (Recommended for now)

```bash
# Install dependencies
bun install

# Run with Ollama
set CLAUDE_CODE_USE_OLLAMA=1
set OLLAMA_MODEL=qwen2.5:latest
bun run ./src/entrypoints/cli.tsx
```

### Option 2: Build Desktop App

```bash
# Install Tauri CLI
cargo install tauri-cli

# Build the desktop app (Tauri project lives in desktop/src-tauri)
cd desktop/src-tauri
cargo tauri build
```

### Run Desktop App

After a successful build, run the executable produced by Cargo/Tauri (path may vary by profile/target).

If you copy the built exe into `desktop/open-claude-code-desktop.exe`, you can run:

```powershell
cd desktop
./open-claude-code-desktop.exe
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CLAUDE_CODE_USE_OLLAMA` | Enable Ollama provider | - |
| `OLLAMA_BASE_URL` | Ollama server URL | http://localhost:11434 |
| `OLLAMA_MODEL` | Default Ollama model | llama3.3 |

## Running with Ollama

1. Make sure Ollama is running: `ollama serve`
2. Start Claude Code with:
   ```bash
   set CLAUDE_CODE_USE_OLLAMA=1
   bun run ./src/entrypoints/cli.tsx
   ```

## Desktop UI notes

- The desktop UI is served from `desktop/dist/index.html`.
- The UI talks to Rust via Tauri `invoke` commands.
- The Tauri global JS API is enabled via `tauri.conf.json` using `app.withGlobalTauri = true`.

## Project Structure

```
desktop/
  src/
    main.rs        - Tauri application entry
    commands.rs    - Rust commands for frontend
  src-tauri/
    Cargo.toml       - Rust dependencies (Tauri project root)
    build.rs         - Tauri build script
    tauri.conf.json  - Tauri configuration
  dist/
    index.html       - Desktop UI
```
