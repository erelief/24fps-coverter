/// Build FFmpeg conversion arguments
pub fn build_command(encoder: &str, input: &std::path::Path, output: &std::path::Path) -> Vec<String> {
    let mut args = vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-r".into(),
        "24".into(),
        "-c:v".into(),
        encoder.into(),
    ];

    // Platform-specific preset handling
    let preset = match encoder {
        "h264_nvenc" => Some("p4"),
        "h264_videotoolbox" => None, // VideoToolbox doesn't support -preset
        _ => Some("medium"),
    };

    if let Some(p) = preset {
        args.extend(["-preset".into(), p.into()]);
    }

    args.extend([
        "-cq".into(),
        "23".into(),
        "-c:a".into(),
        "copy".into(),
        "-c:s".into(),
        "copy".into(),
        "-c:d".into(),
        "copy".into(),
        "-map".into(),
        "0".into(),
        output.to_string_lossy().into_owned(),
    ]);

    args
}
