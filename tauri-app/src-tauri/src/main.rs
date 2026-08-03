// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_home_dir() -> Result<String, String> {
    match dirs::home_dir() {
        Some(path) => Ok(path.to_string_lossy().to_string()),
        None => Err("无法获取用户主目录".to_string()),
    }
}

#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn write_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_dir(path: String) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(&path)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            entry.ok().map(|e| {
                let path = e.path();
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if path.is_dir() {
                    format!("{}/", name)
                } else {
                    name.to_string()
                }
            })
        })
        .collect();
    Ok(entries)
}

#[tauri::command]
fn run_command(command: String, args: Vec<String>, cwd: Option<String>) -> Result<String, String> {
    use std::process::Command;
    
    let mut cmd = Command::new(command);
    cmd.args(args);
    
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    
    let output = cmd.output().map_err(|e| e.to_string())?;
    
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// 在外部编辑器中打开一个「本地」路径（桌面端直接调用，无需走 WebSocket 到后端）
fn open_local_path(path: &str) -> Result<(), String> {
    // 解析编辑器：EDITOR_COMMAND 环境变量 → EDITOR 环境变量 → 平台默认
    let editor = std::env::var("EDITOR_COMMAND")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                "open".to_string()
            } else {
                "xdg-open".to_string()
            }
        });

    if editor.contains("{path}") {
        let cmd_str = editor.replace("{path}", path);
        let (program, args_str) = if cfg!(windows) {
            ("cmd", vec!["/C", &cmd_str])
        } else {
            ("sh", vec!["-c", &cmd_str])
        };
        std::process::Command::new(program)
            .args(args_str)
            .spawn()
            .map_err(|e| format!("Failed to launch editor '{}': {}", editor, e))?;
    } else {
        std::process::Command::new(&editor)
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to launch editor '{}': {}", editor, e))?;
    }
    Ok(())
}

#[tauri::command]
fn open_file_external(path: String) -> Result<(), String> {
    open_local_path(&path)
}

/// 从远端 agent server 下载文件到本地缓存目录，然后用默认编辑器本地打开。
///
/// 场景：Tauri 桌面端连接「远端」服务器（编译系统在远端），文件浏览器里的
/// 文件实际在远端机器上。此命令通过 HTTP 流式下载到 `{cache}/agent-downloads/`，
/// 再复用 `open_file_external` 的编辑器逻辑本地打开。
/// 返回下载到本地的绝对路径。
#[tauri::command]
async fn open_remote_file(url: String, filename: String) -> Result<String, String> {
    // 只保留 basename，防止路径穿越（filename 来自远端 Content-Disposition/条目名）
    let safe_name = std::path::Path::new(&filename)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());

    let cache_dir = dirs::cache_dir().ok_or("无法获取缓存目录")?;
    let dl_dir = cache_dir.join("agent-downloads");
    std::fs::create_dir_all(&dl_dir).map_err(|e| format!("创建下载目录失败: {}", e))?;
    let dest = dl_dir.join(&safe_name);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let mut resp = client.get(&url).send().await
        .map_err(|e| format!("下载失败: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("服务器返回 {}: {}", status, body.trim()));
    }

    // 流式写盘（reqwest bytes_stream，逐块写入，避免整文件进内存）
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(&dest).await
        .map_err(|e| format!("写入本地文件失败: {}", e))?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载中断: {}", e))?;
        file.write_all(&chunk).await
            .map_err(|e| format!("写入本地文件失败: {}", e))?;
    }

    open_local_path(&dest.to_string_lossy())?;
    Ok(dest.to_string_lossy().to_string())
}

// 创建新的项目窗口（Tauri 多窗口隔离）
#[tauri::command]
fn create_project_window(
    app: tauri::AppHandle,
    label: String,
    title: String,
    url: String,
    width: f64,
    height: f64,
    min_width: f64,
    min_height: f64,
) -> Result<(), String> {
    use tauri::WebviewWindowBuilder;
    use tauri::WebviewUrl;

    let win = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title(&title)
        .inner_size(width, height)
        .min_inner_size(min_width, min_height)
        .center()
        .decorations(false)
        .resizable(true)
        .build()
        .map_err(|e| format!("Failed to create window: {}", e))?;

    // Bring to front
    win.show().map_err(|e| format!("Failed to show window: {}", e))?;
    win.set_focus().map_err(|e| format!("Failed to focus window: {}", e))?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            get_home_dir,
            read_file,
            write_file,
            list_dir,
            run_command,
            open_file_external,
            open_remote_file,
            create_project_window
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() {
    run();
}
