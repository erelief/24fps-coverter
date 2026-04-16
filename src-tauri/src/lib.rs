use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

mod converter;
mod context_menu;
mod headless;
mod single_instance;
mod utils;

use converter::ConversionState;

/// Initial file paths passed via command line (right-click context menu)
pub struct StartupInfo {
    pub initial_files: Vec<String>,
}

#[tauri::command]
async fn cmd_convert_files(
    state: State<'_, Arc<Mutex<ConversionState>>>,
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<(), String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;

    if s.is_processing {
        return Err("当前正在处理，请等待完成".into());
    }

    let video_files = utils::collect_video_files(&paths);
    if video_files.is_empty() {
        return Err("未找到支持的视频文件".into());
    }

    let total = video_files.len();
    s.is_processing = true;
    s.stop_requested = false;

    let _ = app.emit(
        "conversion-log",
        serde_json::json!({ "message": format!("添加了 {} 个文件到转换队列", total), "is_error": false }),
    );

    for (i, path) in video_files.iter().enumerate() {
        let _ = app.emit(
            "conversion-log",
            serde_json::json!({ "message": format!("  {}. {}", i + 1, path.file_name().unwrap_or_default().to_string_lossy()), "is_error": false }),
        );
    }

    let ffmpeg_path = s.ffmpeg_path.clone();
    let encoder = s.encoder.clone();
    let state_inner = state.inner().clone();
    let app_clone = app.clone();

    std::thread::spawn(move || {
        let mut success_count = 0usize;
        let mut fail_count = 0usize;

        for (i, input_path) in video_files.iter().enumerate() {
            {
                let s = state_inner.lock().unwrap();
                if s.stop_requested {
                    break;
                }
            }

            let filename = input_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let prefix = if total > 1 {
                format!("[{}/{}] ", i + 1, total)
            } else {
                String::new()
            };
            let _ = app_clone.emit(
                "conversion-log",
                serde_json::json!({ "message": format!("{}正在转换: {}", prefix, filename), "is_error": false }),
            );

            let output_path = utils::get_output_path(input_path);
            let duration = converter::ffmpeg::get_duration(&ffmpeg_path, input_path);

            let result = converter::ffmpeg::convert_with_progress(
                &ffmpeg_path,
                input_path,
                &output_path,
                &encoder,
                duration,
                {
                    let state = state_inner.clone();
                    let app = app_clone.clone();
                    let fname = filename.clone();
                    move |percent| {
                        let _ = app.emit(
                            "conversion-progress",
                            serde_json::json!({ "percent": percent, "filename": fname.clone() }),
                        );
                        let s = state.lock().unwrap();
                        if s.stop_requested {
                            panic!("conversion stopped");
                        }
                    }
                },
                &state_inner,
            );

            match result {
                Ok(msg) => {
                    success_count += 1;
                    let _ = app_clone.emit(
                        "conversion-log",
                        serde_json::json!({ "message": format!("  ✓ {}", msg), "is_error": false }),
                    );
                }
                Err(msg) => {
                    fail_count += 1;
                    let _ = app_clone.emit(
                        "conversion-log",
                        serde_json::json!({ "message": format!("  ✗ {}", msg), "is_error": true }),
                    );
                }
            }
        }

        {
            let mut s = state_inner.lock().unwrap();
            s.is_processing = false;
        }

        let stopped = {
            let s = state_inner.lock().unwrap();
            s.stop_requested
        };

        if stopped {
            let _ = app_clone.emit(
                "conversion-log",
                serde_json::json!({ "message": "转换已停止", "is_error": true }),
            );
        } else {
            let _ = app_clone.emit(
                "conversion-complete",
                serde_json::json!({ "success_count": success_count, "total": success_count + fail_count }),
            );
        }
    });

    Ok(())
}

#[tauri::command]
async fn cmd_cancel_conversion(
    state: State<'_, Arc<Mutex<ConversionState>>>,
) -> Result<(), String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.stop_requested = true;
    s.cancel();
    Ok(())
}

#[tauri::command]
async fn cmd_get_encoder_info(
    state: State<'_, Arc<Mutex<ConversionState>>>,
) -> Result<String, String> {
    let s = state.lock().map_err(|e| e.to_string())?;
    Ok(s.encoder_info.clone())
}

#[tauri::command]
async fn cmd_get_initial_files(
    startup: State<'_, Arc<Mutex<StartupInfo>>>,
) -> Result<Vec<String>, String> {
    let info = startup.lock().map_err(|e| e.to_string())?;
    Ok(info.initial_files.clone())
}

#[tauri::command]
async fn cmd_register_context_menu() -> Result<String, String> {
    let app_path =
        std::env::current_exe().map_err(|e| format!("无法获取应用路径: {}", e))?;
    context_menu::register(&app_path).map_err(|e| e.to_string())?;
    Ok("右键菜单注册成功".into())
}

#[tauri::command]
async fn cmd_unregister_context_menu() -> Result<String, String> {
    context_menu::unregister().map_err(|e| e.to_string())?;
    Ok("右键菜单已注销".into())
}

#[tauri::command]
async fn cmd_is_context_menu_registered() -> Result<bool, String> {
    context_menu::is_registered().map_err(|e| e.to_string())
}

#[tauri::command]
async fn cmd_check_pending_files() -> Result<Option<Vec<String>>, String> {
    Ok(single_instance::read_pending_files())
}

#[tauri::command]
async fn cmd_get_ffmpeg_version(
    state: State<'_, Arc<Mutex<ConversionState>>>,
) -> Result<String, String> {
    let s = state.lock().map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;

    let mut cmd = std::process::Command::new(&s.ffmpeg_path);
    cmd.args(["-version"]);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000);
    }

    let output = cmd.output().map_err(|e| format!("获取 FFmpeg 版本失败: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse "ffmpeg version x.x.x" from first line
    if let Some(line) = stdout.lines().next() {
        if let Some(start) = line.find("version ") {
            let rest = &line[start + 8..];
            let ver = rest.split_whitespace().next().unwrap_or("unknown");
            return Ok(ver.to_string());
        }
    }

    Ok("unknown".into())
}

#[tauri::command]
async fn cmd_get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
async fn cmd_open_about(app: tauri::AppHandle) -> Result<(), String> {
    // If already open, just focus it
    if let Some(win) = app.get_webview_window("about") {
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(&app, "about", tauri::WebviewUrl::App("about.html".into()))
        .title("关于 - 24fps 极速转换器")
        .inner_size(420.0, 200.0)
        .resizable(false)
        .center()
        .decorations(true)
        .build()
        .map_err(|e| format!("打开关于窗口失败: {}", e))?;

    Ok(())
}

#[tauri::command]
async fn cmd_close_about(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("about") {
        win.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn run(initial_files: Vec<String>) {
    let ffmpeg_path = converter::ffmpeg::find_ffmpeg()
        .expect("找不到 FFmpeg！请将 ffmpeg 放入 src-tauri/binaries/ 目录");

    let encoder = converter::encoder::detect_encoder(&ffmpeg_path);
    let encoder_info = converter::encoder::encoder_display_name(&encoder);

    println!("就绪 - 使用 {}", encoder_info);

    let state = Arc::new(Mutex::new(ConversionState {
        ffmpeg_path,
        encoder,
        encoder_info,
        is_processing: false,
        stop_requested: false,
        current_process: None,
    }));

    let startup = Arc::new(Mutex::new(StartupInfo {
        initial_files,
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .manage(startup)
        .invoke_handler(tauri::generate_handler![
            cmd_convert_files,
            cmd_cancel_conversion,
            cmd_get_encoder_info,
            cmd_get_initial_files,
            cmd_register_context_menu,
            cmd_unregister_context_menu,
            cmd_is_context_menu_registered,
            cmd_check_pending_files,
            cmd_get_ffmpeg_version,
            cmd_get_app_version,
            cmd_open_about,
            cmd_close_about,
        ])
        .on_window_event(|window, event| {
            // Close about window when main window closes
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(about) = window.app_handle().get_webview_window("about") {
                    let _ = about.close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
