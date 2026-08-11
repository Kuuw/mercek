mod dbus;
mod capture;
mod wayland;

use crate::dbus::{get_screenshot, DBus};
use crate::capture::load_and_delete;
use image::GenericImageView;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dbus = DBus::new()?;
    let uri = get_screenshot(dbus.connection())?;

    let img = load_and_delete(&uri)?;

    let (width, height) = img.dimensions();
    println!("Image loaded and file deleted. Dimensions: {}x{}", width, height);

    Ok(())
}