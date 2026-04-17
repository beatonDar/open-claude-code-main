use tauri::Manager;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Default)]
struct AppState {
    ollama_url: Mutex<String>,
    model: Mutex<String>,
    messages: Mutex<Vec<Message>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OllamaModel {
    name: String,
    size: u64,
}

#[tauri::command]
async fn check_ollama(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let url = state.ollama_url.lock().unwrap().clone();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    
    let response = client
        .get(format!("{}/api/tags", url))
        .send()
        .await;
    
    match response {
        Ok(resp) => Ok(resp.status().is_success()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn get_ollama_models(state: tauri::State<'_, AppState>) -> Result<Vec<OllamaModel>, String> {
    let url = state.ollama_url.lock().unwrap().clone();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    
    let response = client
        .get(format!("{}/api/tags", url))
        .send()
        .await
        .map_err(|e| format!("Connection error: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }
    
    let json: serde_json::Value = response.json().await.map_err(|e| format!("Parse error: {}", e))?;
    
    let models: Vec<OllamaModel> = json["models"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|m| OllamaModel {
            name: m["name"].as_str().unwrap_or("").to_string(),
            size: m["size"].as_u64().unwrap_or(0),
        })
        .collect();
    
    Ok(models)
}

#[tauri::command]
async fn chat(state: tauri::State<'_, AppState>, message: String) -> Result<String, String> {
    let url = state.ollama_url.lock().unwrap().clone();
    let model = state.model.lock().unwrap().clone();
    
    // Add user message
    {
        let mut messages = state.messages.lock().unwrap();
        messages.push(Message {
            role: "user".to_string(),
            content: message.clone(),
        });
    }
    
    let messages: Vec<Message> = state.messages.lock().unwrap().clone();
    
    let client = reqwest::Client::new();
    let request_body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false
    });
    
    let response = client
        .post(format!("{}/api/chat", url))
        .json(&request_body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    
    if !response.status().is_success() {
        let error = response.text().await.unwrap_or_default();
        return Err(format!("Ollama error: {}", error));
    }
    
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let assistant_message = json["message"]["content"].as_str().unwrap_or("").to_string();
    
    // Add assistant message to history
    {
        let mut messages = state.messages.lock().unwrap();
        messages.push(Message {
            role: "assistant".to_string(),
            content: assistant_message.clone(),
        });
    }
    
    Ok(assistant_message)
}

#[tauri::command]
fn set_ollama_url(state: tauri::State<'_, AppState>, url: String) {
    *state.ollama_url.lock().unwrap() = url;
}

#[tauri::command]
fn set_model(state: tauri::State<'_, AppState>, model: String) {
    *state.model.lock().unwrap() = model;
    // Clear messages when model changes
    state.messages.lock().unwrap().clear();
}

#[tauri::command]
fn clear_chat(state: tauri::State<'_, AppState>) {
    state.messages.lock().unwrap().clear();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            ollama_url: Mutex::new("http://localhost:11434".to_string()),
            model: Mutex::new("llama3.2:1b".to_string()),
            messages: Mutex::new(vec![]),
        })
        .invoke_handler(tauri::generate_handler![
            check_ollama,
            get_ollama_models,
            chat,
            set_ollama_url,
            set_model,
            clear_chat
        ])
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            window.set_title("Open Claude Code - Ollama").ok();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
