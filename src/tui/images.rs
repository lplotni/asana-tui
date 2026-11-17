//! Image attachments drawn in the task pane with the terminal's graphics
//! protocol (Kitty, iTerm2, Sixel, or colored half-blocks as a fallback).

use std::collections::HashMap;

use image::DynamicImage;
use ratatui::Frame;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};

use super::app::{App, ImageState};
use super::ui::ShownImage;

/// Larger downloads are refused.
pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
/// Decoded images are shrunk to fit this many pixels on each side, which is
/// more than a terminal pane shows and keeps memory bounded.
pub const MAX_SIDE: u32 = 1600;

/// Decodes PNG, JPEG, GIF (first frame) or WebP bytes, shrunk to [`MAX_SIDE`].
pub fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    let image = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    if image.width() > MAX_SIDE || image.height() > MAX_SIDE {
        Ok(image.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle))
    } else {
        Ok(image)
    }
}

/// Encodes images for the terminal once per size and draws them.
pub struct ImageCache {
    picker: Picker,
    /// Encoded images by attachment GID, with the area size they were made for.
    encoded: HashMap<String, ((u16, u16), Protocol)>,
}

impl ImageCache {
    pub fn new(picker: Picker) -> ImageCache {
        ImageCache {
            picker,
            encoded: HashMap::new(),
        }
    }

    /// Draws every `shown` image that is loaded, fitted into its area.
    pub fn draw(&mut self, frame: &mut Frame, app: &App, shown: &[ShownImage]) {
        self.encoded.retain(|gid, _| app.images.contains_key(gid));
        for ShownImage {
            attachment_gid,
            area,
        } in shown
        {
            let Some(ImageState::Loaded(image)) = app.images.get(attachment_gid) else {
                continue;
            };
            let size = (area.width, area.height);
            let current = self
                .encoded
                .get(attachment_gid)
                .is_some_and(|(made_for, _)| *made_for == size);
            if !current {
                let target = ratatui::layout::Size::new(area.width, area.height);
                match self
                    .picker
                    .new_protocol((**image).clone(), target, Resize::Fit(None))
                {
                    Ok(protocol) => {
                        self.encoded
                            .insert(attachment_gid.clone(), (size, protocol));
                    }
                    Err(_) => continue,
                }
            }
            if let Some((_, protocol)) = self.encoded.get(attachment_gid) {
                frame.render_widget(Image::new(protocol), *area);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, ImageState};
    use crate::tui::ui::ShownImage;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use std::sync::Arc;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn decodes_a_png() {
        let image = decode(&png(3, 2)).unwrap();

        assert_eq!((image.width(), image.height()), (3, 2));
    }

    #[test]
    fn rejects_bytes_that_are_not_an_image() {
        assert!(decode(b"<html>expired</html>").is_err());
    }

    #[test]
    fn shrinks_large_images_keeping_the_aspect_ratio() {
        let image = decode(&png(MAX_SIDE * 2, MAX_SIDE)).unwrap();

        assert_eq!((image.width(), image.height()), (MAX_SIDE, MAX_SIDE / 2));
    }

    #[test]
    fn draws_a_loaded_image_into_its_area() {
        let mut app = App::default();
        let red = decode(&png(40, 40)).unwrap();
        app.images
            .insert("a".into(), ImageState::Loaded(Arc::new(red)));
        let mut cache = ImageCache::new(ratatui_image::picker::Picker::halfblocks());
        let shown = [ShownImage {
            attachment_gid: "a".into(),
            area: Rect::new(2, 1, 10, 5),
        }];
        let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();

        terminal.draw(|f| cache.draw(f, &app, &shown)).unwrap();

        let buffer = terminal.backend().buffer();
        let red = ratatui::style::Color::Rgb(255, 0, 0);
        let cell = &buffer[(2, 1)];
        assert!(cell.fg == red || cell.bg == red, "{cell:?}");
        assert_eq!(buffer[(0, 0)].symbol(), " ");
    }

    #[test]
    fn draws_nothing_for_an_image_that_is_not_loaded() {
        let mut app = App::default();
        app.images.insert("a".into(), ImageState::Loading);
        let mut cache = ImageCache::new(ratatui_image::picker::Picker::halfblocks());
        let shown = [ShownImage {
            attachment_gid: "a".into(),
            area: Rect::new(0, 0, 10, 5),
        }];
        let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();

        terminal.draw(|f| cache.draw(f, &app, &shown)).unwrap();

        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .all(|c| c.symbol() == " ")
        );
    }
}
