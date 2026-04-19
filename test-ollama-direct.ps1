$body = @{
    model = "llama3.2:1b"
    messages = @(
        @{role = "user"; content = "Hi"}
    )
    stream = $false
    options = @{num_predict = 30}
} | ConvertTo-Json -Depth 3

$response = Invoke-RestMethod -Uri "http://localhost:11434/api/chat" -Method POST -Body $body -ContentType "application/json"
Write-Host "Response time: good"
$response.message.content
