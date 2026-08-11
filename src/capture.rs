use std::fs;
use image::{DynamicImage};

/// Loads the image in the URI to the memory then deletes it
pub fn load_and_delete(uri: &str) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let file_path = uri
        .strip_prefix("file://")
        .ok_or("URI did not have a file:// prefix")?;

    // Load the image into memory using the external `image` crate
    let img = image::open(file_path)?;

    // Delete the temporary file
    fs::remove_file(file_path)?;

    Ok(img)
}