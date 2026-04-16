//! Single-instance mechanism.
//!
//! Windows: named mutex via windows-sys.
//! macOS/Unix: exclusive file lock via flock().
//!
//! When a second instance is launched (e.g., right-click multiple files),
//! file paths are appended to a temp file and the second instance exits.
//! The first instance polls for pending files via `cmd_check_pending_files`.

use std::io::Write;
use std::path::PathBuf;

// =========================================================
// Windows: named mutex
// =========================================================
#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const MUTEX_NAME: &str = "Global\\24fpsConverterSingleInstance";
    static MUTEX_HANDLE: AtomicUsize = AtomicUsize::new(0);

    pub fn is_another_instance_running() -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::System::Threading::CreateMutexW;

        let mut wide_name: Vec<u16> = MUTEX_NAME.encode_utf16().collect();
        wide_name.push(0);

        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 1, wide_name.as_ptr());
            if handle.is_null() {
                return false;
            }
            let last_error = GetLastError();
            if last_error == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return true;
            }
            MUTEX_HANDLE.store(handle as usize, Ordering::SeqCst);
            false
        }
    }

    pub fn bring_existing_to_front() -> Result<(), String> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
        };
        let mut wide_title: Vec<u16> = "24fps 极速转换器".encode_utf16().collect();
        wide_title.push(0);
        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), wide_title.as_ptr());
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
        }
        Ok(())
    }
}

// =========================================================
// macOS / Unix: exclusive file lock via flock()
// =========================================================
#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;

    fn lock_file_path() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push("24fps-converter.lock");
        path
    }

    pub fn is_another_instance_running() -> bool {
        let path = lock_file_path();
        // Open (or create) the lock file
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o666)
            .open(&path)
        {
            Ok(f) => f,
            Err(_) => return false,
        };

        // Try non-blocking exclusive lock
        let fd = file.as_raw_fd();
        let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };

        if result != 0 {
            // Could not acquire lock → another instance holds it
            return true;
        }

        // We hold the lock — leak the file descriptor so it stays locked
        // for the entire process lifetime. OS releases on exit.
        std::mem::forget(file);
        false
    }

    pub fn bring_existing_to_front() -> Result<(), String> {
        // Use osascript to activate the app by name
        let script = r#"
            tell application "System Events"
                set frontmost of every process whose name contains "24fps" to true
            end tell
        "#;
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output();
        Ok(())
    }
}

// =========================================================
// Shared API
// =========================================================

/// Temp file for forwarding paths from secondary → primary instance.
fn pending_file_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push("24fps-converter-pending.txt");
    path
}

/// Check if another instance is already running.
pub fn is_another_instance_running() -> bool {
    imp::is_another_instance_running()
}

/// Bring the existing instance window to the foreground.
pub fn bring_existing_to_front() -> Result<(), String> {
    imp::bring_existing_to_front()
}

/// Append file paths to the pending file (append mode — no overwrite).
pub fn write_pending_files(files: &[String]) -> Result<(), String> {
    let path = pending_file_path();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .map_err(|e| format!("创建待处理文件失败: {}", e))?;

    for f in files {
        writeln!(file, "{}", f).map_err(|e| format!("写入文件路径失败: {}", e))?;
    }

    Ok(())
}

/// Read and clear pending file paths (called by frontend polling).
pub fn read_pending_files() -> Option<Vec<String>> {
    let path = pending_file_path();
    if !path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);

    let files: Vec<String> = content
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.to_string())
        .collect();

    if files.is_empty() {
        None
    } else {
        Some(files)
    }
}
