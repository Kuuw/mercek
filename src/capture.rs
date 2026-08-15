use image::{DynamicImage, ImageFormat};
use std::process::Command;


pub fn get_active_monitor_screenshot() -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let output = Command::new("spectacle")
        .args(["-m", "-b", "-n", "-o", "/dev/stdout"])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Spectacle failed: {err}").into());
    }

    let img = image::load_from_memory_with_format(&output.stdout, ImageFormat::Png)?;
    Ok(img)
}