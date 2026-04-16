use std::path::Path;

/// Encoder priority list per platform
#[cfg(target_os = "windows")]
const ENCODERS: &[&str] = &["h264_nvenc", "h264_qsv", "libx264"];

#[cfg(target_os = "macos")]
const ENCODERS: &[&str] = &["h264_videotoolbox", "libx264"];

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const ENCODERS: &[&str] = &["libx264"];

/// Detect the best available hardware encoder
pub fn detect_encoder(ffmpeg_path: &Path) -> String {
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;

    let mut cmd = std::process::Command::new(ffmpeg_path);
    cmd.args(["-hide_banner", "-encoders"]);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let Ok(output) = cmd.output() else {
        return "libx264".to_string();
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    for encoder in ENCODERS {
        if stdout.contains(encoder) {
            println!("检测到编码器: {}", encoder);
            return encoder.to_string();
        }
    }

    println!("警告: 未检测到硬件编码器，使用 CPU 编码");
    "libx264".to_string()
}

/// Get human-readable encoder name
pub fn encoder_display_name(encoder: &str) -> String {
    match encoder {
        "h264_nvenc" => "NVIDIA NVENC (硬件加速)".into(),
        "h264_qsv" => "Intel Quick Sync (硬件加速)".into(),
        "h264_videotoolbox" => "Apple VideoToolbox (硬件加速)".into(),
        "libx264" => "CPU 软编码".into(),
        _ => encoder.into(),
    }
}
