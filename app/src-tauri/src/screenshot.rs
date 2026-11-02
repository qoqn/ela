//! Saves what a framebuffer Session shows as a PNG, from the exact pixels rather than the
//! canvas, which may be showing a JPEG update.

use std::fs::{self, File};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ela_session::SessionId;

use crate::Manager_;
use crate::error::{CommandError, CommandResult};

#[tauri::command]
#[specta::specta]
pub async fn save_screenshot(
    manager: tauri::State<'_, Manager_>,
    id: SessionId,
) -> CommandResult<String> {
    let (width, height, pixels) = manager
        .snapshot(id)
        .ok_or_else(|| CommandError::new("This session has shown nothing to save yet."))?;
    let dir =
        default_dir().ok_or_else(|| CommandError::new("Can't find a folder for screenshots."))?;
    let path =
        tauri::async_runtime::spawn_blocking(move || save(&dir, width, height, &pixels)).await??;
    Ok(path.display().to_string())
}

fn default_dir() -> Option<PathBuf> {
    dirs::picture_dir()
        .or_else(dirs::home_dir)
        .map(|dir| dir.join("Ela"))
}

fn save(dir: &Path, width: u16, height: u16, pixels: &[u8]) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = dir.join(format!("screenshot-{stamp}.png"));
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(&path)?),
        width.into(),
        height.into(),
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(io::Error::other)?
        .write_image_data(pixels)
        .map_err(io::Error::other)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use super::*;

    #[test]
    fn the_file_holds_exactly_these_pixels() {
        let dir = std::env::temp_dir().join(format!("ela-screenshot-{}", uuid::Uuid::new_v4()));
        let pixels: Vec<u8> = (0..2 * 2 * 4).collect();

        let path = save(&dir, 2, 2, &pixels).unwrap();

        let mut reader = png::Decoder::new(BufReader::new(File::open(&path).unwrap()))
            .read_info()
            .unwrap();
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut decoded).unwrap();
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(&decoded[..info.buffer_size()], &pixels[..]);
        fs::remove_dir_all(dir).unwrap();
    }
}
