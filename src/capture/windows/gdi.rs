//! HellLocker 1.9.7: screen capture for Linux (Wine / Proton), where Windows Graphics
//! Capture doesn't exist. Copies the game's client area from the screen with GDI, so
//! the game has to be visible (it is, while HellLocker works in it). SDR only.
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use image::RgbaImage;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, ClientToScreen,
    CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
    GetDIBits, HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, IsWindow};

use super::color::ColorNormalizer;
use crate::image_rect::ImageRect;
use crate::window::{ClientPoint, WindowTarget};

/// A new frame is at least this far from the last, like a game frame.
const FRAME_SPACING: Duration = Duration::from_millis(16);

pub(super) struct GdiCapture {
    hwnd: HWND,
    size: (u32, u32),
    last: Option<Instant>,
    black_frames: u32,
}

impl GdiCapture {
    pub(super) fn new(target: &WindowTarget) -> Result<Self> {
        let (w, h) = target.client_size();
        if w == 0 || h == 0 {
            bail!("the game window has no client area");
        }
        Ok(Self {
            hwnd: target.native_handle(),
            size: (w, h),
            last: None,
            black_frames: 0,
        })
    }

    pub(super) fn try_reuse(&mut self, target: &WindowTarget) -> bool {
        let alive = unsafe { IsWindow(Some(self.hwnd)) }.as_bool();
        alive && self.hwnd == target.native_handle() && self.size == target.client_size()
    }

    pub(super) fn output_size(&self) -> (u32, u32) {
        self.size
    }

    pub(super) fn map_to_client(&self, roi: ImageRect, local: (u32, u32)) -> ClientPoint {
        ClientPoint {
            x: f64::from(roi.x) + f64::from(local.0),
            y: f64::from(roi.y) + f64::from(local.1),
        }
    }

    pub(super) fn capture_region(
        &mut self,
        roi: ImageRect,
        converter: &ColorNormalizer,
    ) -> Result<RgbaImage> {
        if roi.w == 0 || roi.h == 0 {
            bail!("cannot capture an empty region");
        }
        let mut client = RECT::default();
        unsafe { GetClientRect(self.hwnd, &mut client) }?;
        let (w, h) = (client.right.max(0) as u32, client.bottom.max(0) as u32);
        if (w, h) != self.size {
            bail!(
                "the game window changed size ({}x{} -> {w}x{h})",
                self.size.0,
                self.size.1
            );
        }
        if roi.x + roi.w > w || roi.y + roi.h > h {
            bail!(
                "region ({},{},{},{}) is outside the game window {w}x{h}",
                roi.x,
                roi.y,
                roi.w,
                roi.h
            );
        }
        if let Some(last) = self.last {
            let since = last.elapsed();
            if since < FRAME_SPACING {
                std::thread::sleep(FRAME_SPACING - since);
            }
        }
        self.last = Some(Instant::now());

        let mut origin = POINT {
            x: roi.x as i32,
            y: roi.y as i32,
        };
        if !unsafe { ClientToScreen(self.hwnd, &mut origin) }.as_bool() {
            bail!("could not place the game window on the screen");
        }
        let bgra = copy_screen(origin, roi.w, roi.h)?;

        // A black copy means the screen can't be read (Wayland, say); say so once.
        if bgra.chunks_exact(4).all(|pixel| pixel[..3] == [0, 0, 0]) {
            self.black_frames += 1;
            if self.black_frames == 3 {
                tracing::warn!(
                    "the screen copy of the game is black: on Linux, HellLocker needs an X11 session (not Wayland)"
                );
            }
        } else {
            self.black_frames = 0;
        }

        let row = roi.w as usize * 4;
        let mut out = vec![0u8; row * roi.h as usize];
        for (source, destination) in bgra.chunks_exact(row).zip(out.chunks_exact_mut(row)) {
            converter.convert_bgra8_row(source, destination);
        }
        RgbaImage::from_raw(roi.w, roi.h, out)
            .ok_or_else(|| anyhow::anyhow!("screen copy has the wrong size"))
    }
}

/// Top-down BGRA pixels of a screen rectangle.
fn copy_screen(origin: POINT, w: u32, h: u32) -> Result<Vec<u8>> {
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            bail!("could not open the screen for copying");
        }
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w as i32, h as i32);
        let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
        let copied = BitBlt(
            memory,
            0,
            0,
            w as i32,
            h as i32,
            Some(screen),
            origin.x,
            origin.y,
            SRCCOPY | CAPTUREBLT,
        );
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                // Negative: top-down rows.
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; w as usize * h as usize * 4];
        let lines = GetDIBits(
            memory,
            bitmap,
            0,
            h,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(memory, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        copied?;
        if lines != h as i32 {
            bail!("could not read the screen copy ({lines} of {h} lines)");
        }
        // GDI leaves alpha undefined.
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run by hand with a window titled like the game showing red (left half) and green
    /// (right half): `cargo test gdi_copies_the_game_window -- --ignored`.
    #[test]
    #[ignore]
    fn gdi_copies_the_game_window() {
        let identity = crate::window::WindowIdentity::find_by_title("HELLDIVERS™ 2").unwrap();
        let target = WindowTarget::from_identity(identity).unwrap();
        let capture = GdiCapture::new(&target).unwrap();
        let (w, h) = capture.output_size();
        let mut origin = POINT { x: 0, y: 0 };
        assert!(unsafe { ClientToScreen(capture.hwnd, &mut origin) }.as_bool());
        let pixels = copy_screen(origin, w, h).unwrap();
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [pixels[i + 2], pixels[i + 1], pixels[i]]
        };
        assert_eq!(at(10, h / 2), [200, 30, 30]);
        assert_eq!(at(w - 10, h / 2), [30, 200, 60]);
    }
}
