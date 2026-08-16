mod capture;
mod render;
mod wayland;
mod color_name;

use crate::capture::{get_active_monitor_screenshot};
use image::GenericImageView;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    color_name::init();
    println!("Requesting screenshot from Spectacle...");
    let img = get_active_monitor_screenshot()?;
    let (width, height) = img.dimensions();

    // Convert to RGBA8 pixel buffer for rendering
    let rgba_img = img.to_rgba8();
    let screenshot_rgba = rgba_img.into_raw();

    println!("Launching color picker overlay...");
    let result = wayland::run_overlay(screenshot_rgba, width, height)?;

    match result {
        Some(color) => {
            println!(
                "Selected color: #{:02X}{:02X}{:02X} (RGB: {}, {}, {})",
                color[0], color[1], color[2], color[0], color[1], color[2]
            );
        }
        None => {
            println!("Color picker cancelled.");
        }
    }

    Ok(())
}