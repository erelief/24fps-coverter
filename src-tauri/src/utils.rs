use std::path::{Path, PathBuf};

pub const SUPPORTED_VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "flv", "wmv",
    "webm", "m4v", "mpg", "mpeg", "m2ts", "ts",
];

pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn get_output_path(input: &Path) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    let ext = input.extension().unwrap_or_default().to_string_lossy();
    let parent = input.parent().unwrap_or(Path::new("."));
    parent.join(format!("24fps_{}.{}", stem, ext))
}

pub fn collect_video_files(paths: &[String]) -> Vec<PathBuf> {
    let mut video_files = Vec::new();

    for path_str in paths {
        let trimmed = path_str.trim().trim_matches('"').trim_matches('\'');
        let path = PathBuf::from(trimmed);

        if !path.exists() {
            println!("跳过不存在的路径: {}", path.display());
            continue;
        }

        if path.is_file() {
            if is_video_file(&path) {
                video_files.push(path);
            }
        } else if path.is_dir() {
            collect_from_dir(&path, &mut video_files);
        }
    }

    video_files
}

fn collect_from_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_video_file(&path) {
                files.push(path);
            } else if path.is_dir() {
                collect_from_dir(&path, files);
            }
        }
    }
}

#[allow(dead_code)]
pub fn format_file_size(size_bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size_bytes >= GB {
        format!("{:.1} GB", size_bytes as f64 / GB as f64)
    } else if size_bytes >= MB {
        format!("{:.1} MB", size_bytes as f64 / MB as f64)
    } else if size_bytes >= KB {
        format!("{:.1} KB", size_bytes as f64 / KB as f64)
    } else {
        format!("{} B", size_bytes)
    }
}
