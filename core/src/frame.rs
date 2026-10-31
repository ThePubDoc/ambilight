use crate::config::Rgb;
use crate::layout::Region;

/// A borrowed BGRA8 image as delivered by the capture backends.
pub struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    /// Bytes per row (may include padding).
    pub stride: usize,
    pub data: &'a [u8],
}

impl Frame<'_> {
    #[inline]
    fn pixel(&self, x: usize, y: usize) -> &[u8] {
        let i = y * self.stride + x * 4;
        &self.data[i..i + 4]
    }
}

/// Horizontal and vertical black-bar widths in pixels (applied symmetrically).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Insets {
    pub h: usize,
    pub v: usize,
}

/// Average color of each region, sampled inside the frame minus `insets`.
pub fn sample(frame: &Frame, regions: &[Region], insets: Insets) -> Vec<Rgb> {
    let (w, h) = (frame.width, frame.height);
    if w == 0 || h == 0 {
        return vec![Rgb::BLACK; regions.len()];
    }
    let cw = w.saturating_sub(2 * insets.h).max(1) as f32;
    let ch = h.saturating_sub(2 * insets.v).max(1) as f32;
    let to_x = |f: f32| (insets.h as f32 + f * cw) as usize;
    let to_y = |f: f32| (insets.v as f32 + f * ch) as usize;

    regions
        .iter()
        .map(|r| {
            let x0 = to_x(r.x0).min(w - 1);
            let x1 = (insets.h as f32 + r.x1 * cw).ceil().max((x0 + 1) as f32) as usize;
            let x1 = x1.min(w);
            let y0 = to_y(r.y0).min(h - 1);
            let y1 = (insets.v as f32 + r.y1 * ch).ceil().max((y0 + 1) as f32) as usize;
            let y1 = y1.min(h);
            let (mut sr, mut sg, mut sb) = (0u64, 0u64, 0u64);
            for y in y0..y1 {
                let row = &frame.data[y * frame.stride + x0 * 4..y * frame.stride + x1 * 4];
                for p in row.as_chunks::<4>().0 {
                    sb += p[0] as u64;
                    sg += p[1] as u64;
                    sr += p[2] as u64;
                }
            }
            let n = ((x1 - x0) * (y1 - y0) * 255) as f32;
            Rgb::new(sr as f32 / n, sg as f32 / n, sb as f32 / n)
        })
        .collect()
}

/// Detects letterbox/pillarbox bars. Debounced so a dark scene doesn't make it jump.
#[derive(Debug, Default)]
pub struct BlackBarDetector {
    insets: Insets,
    candidate: Insets,
    candidate_frames: u32,
}

impl BlackBarDetector {
    const DARK: u8 = 20;
    const MAX_FRACTION: usize = 3; // bars may cover up to 30% of each dimension (x/10)
    const STABLE_FRAMES: u32 = 15;

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn detect(&mut self, frame: &Frame) -> Insets {
        let (w, h) = (frame.width, frame.height);
        let dark = |x: usize, y: usize| frame.pixel(x, y)[..3].iter().all(|&c| c < Self::DARK);
        let column_dark = |x: usize| (0..h).step_by(2).all(|y| dark(x, y));
        let row_dark = |y: usize| (0..w).step_by(2).all(|x| dark(x, y));

        let max_h = w * Self::MAX_FRACTION / 10;
        let max_v = h * Self::MAX_FRACTION / 10;
        let count = |max: usize, is_dark: &dyn Fn(usize) -> bool| (0..max).take_while(|&i| is_dark(i)).count();
        let l = count(max_h, &|i| column_dark(i));
        let r = count(max_h, &|i| column_dark(w - 1 - i));
        let t = count(max_v, &|i| row_dark(i));
        let b = count(max_v, &|i| row_dark(h - 1 - i));

        // A fully dark frame (fade to black) says nothing about bars.
        if l >= max_h && t >= max_v {
            return self.insets;
        }
        let pick = |a: usize, b: usize, max: usize| {
            let v = a.min(b);
            if v >= max || v < 2 {
                0
            } else {
                v
            }
        };
        let found = Insets { h: pick(l, r, max_h), v: pick(t, b, max_v) };

        if found == self.candidate {
            self.candidate_frames += 1;
        } else {
            self.candidate = found;
            self.candidate_frames = 1;
        }
        if self.candidate_frames >= Self::STABLE_FRAMES {
            self.insets = self.candidate;
        }
        self.insets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::layout;

    /// BGRA image where `f(x, y)` gives RGB.
    fn image(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let [r, g, b] = f(x, y);
                v.extend_from_slice(&[b, g, r, 255]);
            }
        }
        v
    }

    #[test]
    fn samples_left_and_right_halves() {
        let (w, h) = (200, 100);
        let data = image(w, h, |x, _| if x < w / 2 { [255, 0, 0] } else { [0, 0, 255] });
        let frame = Frame { width: w, height: h, stride: w * 4, data: &data };
        let c = Config { top: 2, right: 1, bottom: 0, left: 1, ..Config::default() };
        let regions = layout::build(&c);
        let colors = sample(&frame, &regions, Insets::default());
        for (r, c) in regions.iter().zip(&colors) {
            let expect_red = r.x1 <= 0.5;
            assert_eq!(c.r > 0.99, expect_red, "{r:?} -> {c:?}");
            assert_eq!(c.b > 0.99, !expect_red, "{r:?} -> {c:?}");
        }
    }

    #[test]
    fn detects_pillarbox_after_debounce() {
        let (w, h) = (320, 134);
        let data = image(w, h, |x, _| if x < 40 || x >= w - 40 { [0, 0, 0] } else { [200, 120, 40] });
        let frame = Frame { width: w, height: h, stride: w * 4, data: &data };
        let mut d = BlackBarDetector::default();
        for _ in 0..BlackBarDetector::STABLE_FRAMES - 1 {
            assert_eq!(d.detect(&frame), Insets::default());
        }
        assert_eq!(d.detect(&frame), Insets { h: 40, v: 0 });
    }

    #[test]
    fn black_frame_keeps_previous_insets() {
        let (w, h) = (100, 50);
        let data = image(w, h, |_, _| [0, 0, 0]);
        let frame = Frame { width: w, height: h, stride: w * 4, data: &data };
        let mut d = BlackBarDetector::default();
        assert_eq!(d.detect(&frame), Insets::default());
    }

    #[test]
    fn respects_row_padding() {
        let (w, h, stride) = (4, 2, 32);
        let mut data = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                data[y * stride + x * 4 + 1] = 255; // green
            }
        }
        let frame = Frame { width: w, height: h, stride, data: &data };
        let c = Config { top: 1, right: 0, bottom: 0, left: 0, ..Config::default() };
        let colors = sample(&frame, &layout::build(&c), Insets::default());
        assert_eq!(colors[0], Rgb::new(0.0, 1.0, 0.0));
    }
}
