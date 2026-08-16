use std::io::Cursor;
use std::process::Command;

pub struct Rgba8Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub fn get_active_monitor_screenshot() -> Result<Rgba8Image, Box<dyn std::error::Error>> {
    let output = Command::new("spectacle")
        .args(["-m", "-b", "-n", "-o", "/dev/stdout"])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Spectacle failed: {err}").into());
    }

    let mut decoder = png::Decoder::new(Cursor::new(&output.stdout));
    // Normalizes palette/indexed, adds alpha channel if missing, and strips 16-bit to 8-bit
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::STRIP_16
    );

    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf)?;
    buf.truncate(info.buffer_size());

    // If source was RGB without Alpha, convert to RGBA
    let data = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
            for chunk in buf.chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
            for &g in &buf {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
            for chunk in buf.chunks_exact(2) {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            rgba
        }
        _ => return Err("Unsupported color format".into()),
    };

    Ok(Rgba8Image {
        width: info.width,
        height: info.height,
        data,
    })
}