use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::command_builder;

/// Find FFmpeg binary: sidecar location > system PATH
pub fn find_ffmpeg() -> Result<PathBuf, String> {
    // 1. Sidecar: bundled with the app via Tauri externalBin
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Tauri sidecar naming: ffmpeg-{target-triple}[.exe]
            if let Ok(entries) = std::fs::read_dir(exe_dir) {
                for entry in entries.flatten() {
                    let name_str = entry.file_name().to_string_lossy().to_string();
                    if name_str.starts_with("ffmpeg-")
                        && (name_str.ends_with(".exe") || !name_str.contains('.'))
                    {
                        let path = entry.path();
                        if path.is_file() {
                            println!("使用 sidecar FFmpeg: {}", path.display());
                            return Ok(path);
                        }
                    }
                }
            }

            // Also check for plain ffmpeg in the exe directory
            let ffmpeg_name = if cfg!(target_os = "windows") {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            };
            let local = exe_dir.join(ffmpeg_name);
            if local.is_file() {
                println!("使用本地 FFmpeg: {}", local.display());
                return Ok(local);
            }
        }
    }

    // 2. Development mode: check project binaries/ directory
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let bin_dir = PathBuf::from(manifest_dir).join("binaries");
        if bin_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&bin_dir) {
                for entry in entries.flatten() {
                    let name_str = entry.file_name().to_string_lossy().to_string();
                    if name_str.starts_with("ffmpeg-")
                        || name_str == "ffmpeg.exe"
                        || name_str == "ffmpeg"
                    {
                        let path = entry.path();
                        if path.is_file() {
                            println!("使用开发 FFmpeg: {}", path.display());
                            return Ok(path);
                        }
                    }
                }
            }
        }
    }

    // 3. System PATH
    if let Ok(which) = which_ffmpeg() {
        println!("使用系统 FFmpeg: {}", which.display());
        return Ok(which);
    }

    Err("找不到 FFmpeg！请将 ffmpeg 放入 src-tauri/binaries/ 目录或添加到系统 PATH".into())
}

fn which_ffmpeg() -> Result<PathBuf, String> {
    let name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    let output = std::process::Command::new("where").arg(name).output();

    match output {
        Ok(o) if o.status.success() => {
            let path = String::from_utf8_lossy(&o.stdout);
            let first_line = path.lines().next().unwrap_or("").trim();
            if !first_line.is_empty() {
                return Ok(PathBuf::from(first_line));
            }
        }
        _ => {}
    }

    #[cfg(unix)]
    {
        if let Ok(o) = std::process::Command::new("which").arg("ffmpeg").output() {
            if o.status.success() {
                let path = String::from_utf8_lossy(&o.stdout);
                let first_line = path.lines().next().unwrap_or("").trim();
                if !first_line.is_empty() {
                    return Ok(PathBuf::from(first_line));
                }
            }
        }
    }

    Err("ffmpeg not found in PATH".into())
}

/// Get video duration in seconds
pub fn get_duration(ffmpeg_path: &Path, input: &Path) -> f64 {
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;

    let mut cmd = std::process::Command::new(ffmpeg_path);
    cmd.args(["-hide_banner", "-i"]).arg(input);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = cmd.output();

    if let Ok(output) = output {
        let stderr = String::from_utf8_lossy(&output.stderr);
        for line in stderr.lines() {
            if let Some(pos) = line.find("Duration:") {
                let duration_str = line[pos + 9..].split(',').next().unwrap_or("").trim();
                let parts: Vec<&str> = duration_str.split(':').collect();
                if parts.len() == 3 {
                    if let (Ok(h), Ok(m), Ok(s)) =
                        (parts[0].parse::<f64>(), parts[1].parse::<f64>(), parts[2].parse::<f64>())
                    {
                        return h * 3600.0 + m * 60.0 + s;
                    }
                }
            }
        }
    }

    0.0
}

/// Convert a video file with progress callback.
pub fn convert_with_progress<F>(
    ffmpeg_path: &Path,
    input: &Path,
    output: &Path,
    encoder: &str,
    duration: f64,
    progress_callback: F,
    state: &Arc<Mutex<crate::converter::ConversionState>>,
) -> Result<String, String>
where
    F: Fn(f32) + Send + 'static,
{
    if !input.exists() {
        return Err(format!("输入文件不存在: {}", input.display()));
    }

    let mut args = command_builder::build_command(encoder, input, output);
    // Insert -progress pipe:1 before the output path
    let last = args.pop().unwrap(); // output path
    args.extend(["-progress".into(), "pipe:1".into()]);
    args.push(last);

    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;

    let mut cmd = std::process::Command::new(ffmpeg_path);
    cmd.args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    {
        // CREATE_NO_WINDOW = 0x08000000 — prevents console window from appearing
        cmd.creation_flags(0x08000000);
    }

    let child = cmd.spawn()
        .map_err(|e| format!("启动 FFmpeg 失败: {}", e))?;

    // Store process handle for cancellation
    {
        let mut s = state.lock().unwrap();
        s.current_process = Some(child);
    }

    // Take stdout before storing child back
    let stdout = {
        let mut s = state.lock().unwrap();
        s.current_process.as_mut().and_then(|c| c.stdout.take())
    };

    if let Some(stdout) = stdout {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stdout);
        let duration_us = duration * 1_000_000.0;

        for line_result in reader.lines() {
            // Check stop flag between lines
            {
                let s = state.lock().unwrap();
                if s.stop_requested {
                    break;
                }
            }

            if let Ok(line) = line_result {
                let line = line.trim();
                if line.starts_with("out_time_us=") {
                    if let Ok(time_us) = line[12..].parse::<f64>() {
                        if duration_us > 0.0 {
                            let pct = (time_us / duration_us * 100.0).min(100.0) as f32;
                            progress_callback(pct);
                        } else {
                            progress_callback(-1.0);
                        }
                    }
                }
            } else {
                break;
            }
        }
    }

    // Wait for process to finish and get exit code
    let exit_status = {
        let mut s = state.lock().unwrap();
        if let Some(ref mut child) = s.current_process {
            child.wait().ok()
        } else {
            None
        }
    };

    // Clean up
    {
        let mut s = state.lock().unwrap();
        s.current_process = None;
    }

    match exit_status {
        Some(status) if status.success() => {
            if output.exists() {
                Ok(format!("转换成功: {}", output.display()))
            } else {
                Err("转换完成但输出文件不存在".into())
            }
        }
        Some(status) => {
            let _ = std::fs::remove_file(output);
            Err(format!(
                "转换失败 (exit code {})",
                status.code().unwrap_or(-1)
            ))
        }
        None => {
            let _ = std::fs::remove_file(output);
            Err("转换进程异常终止".into())
        }
    }
}
