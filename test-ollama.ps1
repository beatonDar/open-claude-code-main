$body = @{
    model = "qwen2.5:latest"
    messages = @(
        @{role = "user"; content = "Hi"}
    )
    stream = $false
    options = @{num_predict = 50}
} | ConvertTo-Json -Depth 3

$result = Invoke-RestMethod -Uri "http://localhost:11434/api/chat" -Method POST -Body $body -ContentType "application/json"
$result.message.content
