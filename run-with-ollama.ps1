$env:CLAUDE_CODE_USE_OLLAMA = "1"
$env:OLLAMA_FORCE_ARABIC = "0"
$env:OLLAMA_MODEL = "llama3.2:1b"
$env:OLLAMA_BASE_URL = "http://localhost:11434"
bun run ./src/entrypoints/cli.tsx -- --bare

