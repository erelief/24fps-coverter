#![allow(dead_code)]

mod private {
    pub trait ContextMenuManager {
        fn register(app_path: &std::path::Path) -> Result<(), String>;
        fn unregister() -> Result<(), String>;
        fn is_registered() -> Result<bool, String>;
    }
}

pub use private::ContextMenuManager;

const VIDEO_EXTENSIONS: &[&str] = &[
    ".mp4", ".mkv", ".avi", ".mov", ".flv", ".wmv",
    ".webm", ".m4v", ".mpg", ".mpeg", ".m2ts", ".ts",
];

#[cfg(target_os = "windows")]
mod platform {
    use super::*;

    pub struct WindowsContextMenu;

    impl super::ContextMenuManager for WindowsContextMenu {
        fn register(app_path: &std::path::Path) -> Result<(), String> {
            use winreg::enums::*;
            use winreg::RegKey;

            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let app_path_str = app_path.to_string_lossy().to_string();
            let command = format!(r#""{}" "%1""#, app_path_str);

            // Register under each video extension's SystemFileAssociations
            // This is the most reliable way to show context menu items
            // MultiSelectModel helps on some Windows versions; single-instance
            // forwarding handles the rest
            for ext in VIDEO_EXTENSIONS {
                let key_path = format!(
                    r"Software\Classes\SystemFileAssociations\{}\shell\24fpsConvert",
                    ext
                );

                let (key, _) = hkcu
                    .create_subkey(&key_path)
                    .map_err(|e| format!("注册表写入失败: {}", e))?;

                key.set_value("", &"Convert to 24fps")
                    .map_err(|e| format!("注册表写入失败: {}", e))?;
                key.set_value("Icon", &app_path_str.as_str())
                    .map_err(|e| format!("注册表写入失败: {}", e))?;
                key.set_value("MultiSelectModel", &"Player")
                    .map_err(|e| format!("注册表写入失败: {}", e))?;

                let (cmd_key, _) = hkcu
                    .create_subkey(format!(r"{}\command", key_path))
                    .map_err(|e| format!("注册表写入失败: {}", e))?;

                cmd_key
                    .set_value("", &command.as_str())
                    .map_err(|e| format!("注册表写入失败: {}", e))?;
            }

            Ok(())
        }

        fn unregister() -> Result<(), String> {
            use winreg::enums::*;
            use winreg::RegKey;

            let hkcu = RegKey::predef(HKEY_CURRENT_USER);

            for ext in VIDEO_EXTENSIONS {
                let key_path = format!(
                    r"Software\Classes\SystemFileAssociations\{}\shell\24fpsConvert",
                    ext
                );
                let _ = hkcu.delete_subkey_all(&key_path);
            }

            // Also clean up * class entries from previous attempt
            let _ = hkcu.delete_subkey_all(r"Software\Classes\*\shell\24fpsConvert");

            Ok(())
        }

        fn is_registered() -> Result<bool, String> {
            use winreg::enums::*;
            use winreg::RegKey;

            let hkcu = RegKey::predef(HKEY_CURRENT_USER);

            // Check SystemFileAssociations first (primary method)
            match hkcu.open_subkey(
                r"Software\Classes\SystemFileAssociations\.mp4\shell\24fpsConvert",
            ) {
                Ok(_) => Ok(true),
                Err(_) => {
                    // Fallback: check * class location
                    match hkcu.open_subkey(r"Software\Classes\*\shell\24fpsConvert") {
                        Ok(_) => Ok(true),
                        Err(_) => Ok(false),
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub struct MacOSContextMenu;

    impl super::ContextMenuManager for MacOSContextMenu {
        fn register(app_path: &std::path::Path) -> Result<(), String> {
            let home = std::env::var("HOME").map_err(|e| format!("无法获取 HOME: {}", e))?;
            let services_dir = std::path::PathBuf::from(home)
                .join("Library")
                .join("Services");
            std::fs::create_dir_all(&services_dir)
                .map_err(|e| format!("创建 Services 目录失败: {}", e))?;

            let workflow_dir = services_dir.join("Convert to 24fps.workflow");
            let contents_dir = workflow_dir.join("Contents");
            std::fs::create_dir_all(&contents_dir)
                .map_err(|e| format!("创建 workflow 目录失败: {}", e))?;

            // Write Info.plist
            let app_path_str = app_path.to_string_lossy().to_string();
            let info_plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Convert to 24fps</string>
    <key>CFBundleIdentifier</key>
    <string>com.24fps-converter.quickaction</string>
    <key>CFBundleVersion</key>
    <string>1.0</string>
    <key>NSServices</key>
    <array>
        <dict>
            <key>NSMenuItem</key>
            <dict>
                <key>default</key>
                <string>Convert to 24fps</string>
            </dict>
            <key>NSMessage</key>
            <string>runWorkflowAsService</string>
            <key>NSRequiredContext</key>
            <dict>
                <key>NSApplicationIdentifier</key>
                <string>com.apple.finder</string>
            </dict>
            <key>NSSendFileTypes</key>
            <array>
                <string>public.mpeg-4</string>
                <string>com.apple.quicktime-movie</string>
                <string>public.avi</string>
                <string>public.movie</string>
            </array>
        </dict>
    </array>
</dict>
</plist>"#
            );

            std::fs::write(contents_dir.join("Info.plist"), info_plist)
                .map_err(|e| format!("写入 Info.plist 失败: {}", e))?;

            // Write document.wflow with a shell script action
            let escaped_path = app_path_str.replace(' ', "\\ ");
            let workflow = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>AMApplicationBuild</key><string>523</string>
    <key>AMApplicationVersion</key><string>2.10</string>
    <key>AMDocumentVersion</key><string>2</string>
    <key>actions</key>
    <array>
        <dict>
            <key>action</key>
            <dict>
                <key>AMAccepts</key>
                <dict>
                    <key>Container</key><string>COMPOSITE</string>
                    <key>Optional</key><true/>
                    <key>Types</key>
                    <array><string>com.apple.cocoa.path</string></array>
                </dict>
                <key>AMActionVersion</key><string>2.0.3</string>
                <key>AMApplication</key><array><string>Automator</string></array>
                <key>AMCategory</key><string>AMCategoryUtilities</string>
                <key>AMName</key><string>Run Shell Script</string>
                <key>AMProvides</key>
                <dict>
                    <key>Container</key><string>COMPOSITE</string>
                    <key>Types</key>
                    <array><string>com.apple.cocoa.path</string></array>
                </dict>
                <key>ActionBundlePath</key>
                <string>/System/Library/Automator/Run Shell Script.action</string>
                <key>ActionName</key><string>Run Shell Script</string>
                <key>ActionParameters</key>
                <dict>
                    <key>COMMAND_STRING</key>
                    <string>{escaped_path} "$@"</string>
                    <key>CheckedForUserDefaultShell</key><true/>
                    <key>inputMethod</key><integer>1</integer>
                    <key>shell</key><string>/bin/bash</string>
                    <key>source</key><string></string>
                </dict>
                <key>BundleIdentifier</key>
                <string>com.apple.RunShellScript</string>
                <key>CFBundleVersion</key><string>2.0.3</string>
            </dict>
        </dict>
    </array>
    <key>connectors</key><dict/>
    <key>workflowMetaData</key>
    <dict>
        <key>workflowTypeIdentifier</key>
        <string>com.apple.Automator.servicesMenu</string>
    </dict>
</dict>
</plist>"#
            );

            std::fs::write(contents_dir.join("document.wflow"), workflow)
                .map_err(|e| format!("写入 document.wflow 失败: {}", e))?;

            // Refresh services
            let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
                .arg("flush")
                .output();

            Ok(())
        }

        fn unregister() -> Result<(), String> {
            let home = std::env::var("HOME").map_err(|e| format!("无法获取 HOME: {}", e))?;
            let workflow = std::path::PathBuf::from(home)
                .join("Library")
                .join("Services")
                .join("Convert to 24fps.workflow");

            if workflow.exists() {
                std::fs::remove_dir_all(&workflow)
                    .map_err(|e| format!("删除 workflow 失败: {}", e))?;
            }

            Ok(())
        }

        fn is_registered() -> Result<bool, String> {
            let home = std::env::var("HOME").map_err(|e| format!("无法获取 HOME: {}", e))?;
            let workflow = std::path::PathBuf::from(home)
                .join("Library")
                .join("Services")
                .join("Convert to 24fps.workflow");

            Ok(workflow.exists())
        }
    }
}

// Dispatch to platform implementation
pub fn register(app_path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        platform::WindowsContextMenu::register(app_path)
    }
    #[cfg(target_os = "macos")]
    {
        platform::MacOSContextMenu::register(app_path)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = app_path;
        Err("当前平台不支持右键菜单注册".into())
    }
}

pub fn unregister() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        platform::WindowsContextMenu::unregister()
    }
    #[cfg(target_os = "macos")]
    {
        platform::MacOSContextMenu::unregister()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err("当前平台不支持右键菜单注销".into())
    }
}

pub fn is_registered() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        platform::WindowsContextMenu::is_registered()
    }
    #[cfg(target_os = "macos")]
    {
        platform::MacOSContextMenu::is_registered()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(false)
    }
}
