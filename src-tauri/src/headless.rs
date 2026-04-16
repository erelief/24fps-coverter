#![allow(dead_code)]

use crate::converter;
use crate::utils;

pub fn run(files: &[String]) {
    let video_files = utils::collect_video_files(files);

    if video_files.is_empty() {
        notify_headless("24fps 转换器", "未找到支持的视频文件");
        std::process::exit(1);
    }

    let ffmpeg_path = match converter::ffmpeg::find_ffmpeg() {
        Ok(p) => p,
        Err(e) => {
            notify_headless("24fps 转换器", &format!("初始化失败: {}", e));
            std::process::exit(1);
        }
    };

    let encoder = converter::encoder::detect_encoder(&ffmpeg_path);
    let total = video_files.len();
    let mut success_count = 0usize;
    let mut fail_count = 0usize;

    for input_path in &video_files {
        let output_path = utils::get_output_path(input_path);
        let _duration = converter::ffmpeg::get_duration(&ffmpeg_path, input_path);

        // Simple synchronous conversion (no progress callback in headless)
        let result = simple_convert(&ffmpeg_path, &input_path, &output_path, &encoder);

        match result {
            Ok(_) => success_count += 1,
            Err(_) => fail_count += 1,
        }
    }

    if fail_count == 0 {
        notify_headless(
            "24fps 转换器",
            &format!("{} 个文件已成功转换为 24fps", total),
        );
    } else {
        notify_headless(
            "24fps 转换器",
            &format!("{}/{} 成功", success_count, total),
        );
        std::process::exit(1);
    }
}

fn simple_convert(
    ffmpeg_path: &std::path::Path,
    input: &std::path::Path,
    output: &std::path::Path,
    encoder: &str,
) -> Result<(), String> {
    let args = converter::command_builder::build_command(encoder, input, output);

    let result = std::process::Command::new(ffmpeg_path)
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match result {
        Ok(status) if status.success() => {
            if output.exists() {
                Ok(())
            } else {
                Err("输出文件不存在".into())
            }
        }
        Ok(status) => Err(format!("转换失败 (exit code {})", status.code().unwrap_or(-1))),
        Err(e) => Err(format!("启动 FFmpeg 失败: {}", e)),
    }
}

fn notify_headless(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        let title_esc = title.replace('"', "\\\"");
        let body_esc = body.replace('"', "\\\"").replace('\n', "\\n");
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            body_esc, title_esc
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        let title_esc = title.replace('\'', "''");
        let body_esc = body.replace('\'', "''").replace('\n', "`n");
        let ps_script = format!(
            r#"
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom, ContentType = WindowsRuntime] | Out-Null
$template = @"
<toast>
  <visual>
    <binding template="ToastGeneric">
      <text>{title}</text>
      <text>{body}</text>
    </binding>
  </visual>
  <audio silent="true"/>
</toast>
"@
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml($template)
$notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("24fps Converter")
$toast = New-Object Windows.UI.Notifications.ToastNotification $xml
$notifier.Show($toast)
"#,
            title = title_esc,
            body = body_esc
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output();
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .show();
    }
}
