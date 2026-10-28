//! Headless rendering with the software renderer: no window, no GPU, same bytes every run.

use std::error::Error;
use std::path::Path;
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{PhysicalSize, Rgb8Pixel};

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

/// Install the headless platform. Call once, before creating any component.
pub fn install(width: u32, height: u32) -> Result<Rc<MinimalSoftwareWindow>, Box<dyn Error>> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(PhysicalSize::new(width, height));
    slint::platform::set_platform(Box::new(Headless(window.clone())))
        .map_err(|e| format!("cannot install the headless platform: {e}"))?;
    Ok(window)
}

/// Render the window's current contents and write them as a PNG.
pub fn save(
    window: &MinimalSoftwareWindow,
    width: u32,
    height: u32,
    out: &Path,
) -> Result<(), Box<dyn Error>> {
    slint::platform::update_timers_and_animations();
    let mut pixels = vec![Rgb8Pixel::default(); (width * height) as usize];
    let drawn = window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, width as usize);
    });
    if !drawn {
        return Err("nothing was drawn".into());
    }
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    let img = image::RgbImage::from_raw(width, height, bytes).ok_or("bad pixel buffer")?;
    img.save_with_format(out, image::ImageFormat::Png)?;
    Ok(())
}
