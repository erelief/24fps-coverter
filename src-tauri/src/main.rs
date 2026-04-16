// Always use Windows subsystem — no console window flash
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod converter;
mod context_menu;
mod headless;
mod single_instance;
mod utils;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Legacy headless (CLI usage)
    if args.iter().any(|a| a == "--headless") {
        let files: Vec<String> = args[1..]
            .iter()
            .filter(|a| *a != "--headless")
            .cloned()
            .collect();
        headless::run(&files);
        return;
    }

    // Collect file paths from args (everything except the exe name)
    let files: Vec<String> = args[1..]
        .iter()
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect();

    // Single-instance: if another instance is running, forward files to it
    if single_instance::is_another_instance_running() {
        if !files.is_empty() {
            let _ = single_instance::write_pending_files(&files);
        }
        let _ = single_instance::bring_existing_to_front();
        return;
    }

    fps_converter_lib::run(files);
}
