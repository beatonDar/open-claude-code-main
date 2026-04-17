$env:CLAUDE_CODE_USE_OLLAMA = "1"
$env:OLLAMA_MODEL = "qwen2.5:latest"
$env:OLLAMA_BASE_URL = "http://localhost:11434"
& "C:\Users\sd\.bun\bin\bun.exe" run ./src/entrypoints/cli.tsx -- --bare
