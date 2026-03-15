use std::process::{Child, Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

// Python 播放器进程
lazy_static::lazy_static! {
    static ref PYTHON_PROCESS: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
}

/// 启动 Python 播放器进程
fn ensure_python_player() -> Result<(), String> {
    let mut guard = PYTHON_PROCESS.lock().unwrap();
    
    if guard.is_none() || guard.as_ref().map(|p| p.try_wait().ok().flatten().is_some()).unwrap_or(true) {
        // Python 脚本路径（相对于应用目录）
        let python_script = std::env::current_dir()
            .map_err(|e| format!("Failed to get current dir: {}", e))?
            .join("python")
            .join("audio_player.py");
        
        let child = Command::new("python3")
            .arg(&python_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start Python player: {}. Is Python installed?", e))?;
        
        *guard = Some(child);
        log::info!("[PythonPlayer] Started");
    }
    
    Ok(())
}

/// 发送命令到 Python 播放器
fn send_command(cmd: &str) -> Result<String, String> {
    ensure_python_player()?;
    
    let mut guard = PYTHON_PROCESS.lock().unwrap();
    if let Some(ref mut child) = *guard {
        // 发送命令
        if let Some(ref mut stdin) = child.stdin {
            writeln!(stdin, "{}", cmd).map_err(|e| format!("Failed to write: {}", e))?;
            stdin.flush().map_err(|e| format!("Failed to flush: {}", e))?;
        }
        
        // 读取响应
        if let Some(ref mut stdout) = child.stdout {
            let mut reader = BufReader::new(stdout);
            let mut response = String::new();
            reader.read_line(&mut response)
                .map_err(|e| format!("Failed to read: {}", e))?;
            return Ok(response.trim().to_string());
        }
    }
    
    Err("Python player not available".to_string())
}

/// 播放音频
#[tauri::command]
pub async fn python_audio_play(url: String) -> Result<String, String> {
    log::info!("[PythonPlayer] Playing: {}", url);
    
    let cmd = serde_json::json!({
        "action": "play",
        "url": url
    });
    
    let response = send_command(&cmd.to_string())?;
    Ok(response)
}

/// 暂停
#[tauri::command]
pub fn python_audio_pause() -> Result<String, String> {
    log::info!("[PythonPlayer] Pause");
    
    let cmd = serde_json::json!({"action": "pause"});
    send_command(&cmd.to_string())
}

/// 继续
#[tauri::command]
pub fn python_audio_resume() -> Result<String, String> {
    log::info!("[PythonPlayer] Resume");
    
    let cmd = serde_json::json!({"action": "resume"});
    send_command(&cmd.to_string())
}

/// 停止
#[tauri::command]
pub fn python_audio_stop() -> Result<String, String> {
    log::info!("[PythonPlayer] Stop");
    
    let cmd = serde_json::json!({"action": "stop"});
    let result = send_command(&cmd.to_string());
    
    // 停止 Python 进程
    {
        let mut guard = PYTHON_PROCESS.lock().unwrap();
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    
    result
}

/// 获取状态
#[tauri::command]
pub fn python_audio_get_state() -> Result<serde_json::Value, String> {
    let cmd = serde_json::json!({"action": "getState"});
    let response = send_command(&cmd.to_string())?;
    
    serde_json::from_str(&response)
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// 设置音量
#[tauri::command]
pub fn python_audio_set_volume(volume: f32) -> Result<String, String> {
    log::info!("[PythonPlayer] Set volume: {}", volume);
    
    let cmd = serde_json::json!({
        "action": "setVolume",
        "volume": volume
    });
    
    send_command(&cmd.to_string())
}
