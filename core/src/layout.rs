use crate::config::{Config, Corner, Direction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

/// Screen area one LED samples, normalized to 0..=1 with the origin at the top-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub side: Side,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// Side LEDs look this much less deep (as a fraction of width) than top/bottom LEDs do (as a
/// fraction of height), so both sample a similar number of pixels on a typical wide screen.
const SIDE_DEPTH_RATIO: f32 = 0.625;

/// Regions in the order the LEDs are wired.
pub fn build(c: &Config) -> Vec<Region> {
    let depth = c.edge_depth.clamp(1.0, 40.0) / 100.0;
    let side_depth = depth * SIDE_DEPTH_RATIO;
    let span = |i: u16, n: u16| (i as f32 / n as f32, (i + 1) as f32 / n as f32);

    // Clockwise from the top-left corner: top L→R, right T→B, bottom R→L, left B→T.
    let mut cw = Vec::with_capacity(c.led_count());
    for i in 0..c.top {
        let (s, e) = span(i, c.top);
        cw.push(Region { side: Side::Top, x0: s, y0: 0.0, x1: e, y1: depth });
    }
    for i in 0..c.right {
        let (s, e) = span(i, c.right);
        cw.push(Region { side: Side::Right, x0: 1.0 - side_depth, y0: s, x1: 1.0, y1: e });
    }
    // Bottom LEDs fill the edge except for `bottom_gap` LED-sized slots in the middle.
    let slots = c.bottom + c.bottom_gap;
    let left_half = c.bottom / 2;
    for i in (0..c.bottom).rev() {
        let slot = if i < left_half { i } else { i + c.bottom_gap };
        let (s, e) = span(slot, slots);
        cw.push(Region { side: Side::Bottom, x0: s, y0: 1.0 - depth, x1: e, y1: 1.0 });
    }
    for i in (0..c.left).rev() {
        let (s, e) = span(i, c.left);
        cw.push(Region { side: Side::Left, x0: 0.0, y0: s, x1: side_depth, y1: e });
    }
    if cw.is_empty() {
        return cw;
    }

    let len = cw.len();
    let corner = match c.start {
        Corner::TopLeft => 0,
        Corner::TopRight => c.top as usize,
        Corner::BottomRight => (c.top + c.right) as usize,
        Corner::BottomLeft => (c.top + c.right + c.bottom) as usize,
    };
    cw.rotate_left(corner % len);
    // Going anticlockwise from a corner is the clockwise run that ends there, reversed.
    if c.direction == Direction::Anticlockwise {
        cw.reverse();
    }
    // LED 1 sits `offset` LEDs further along the strip than the corner.
    cw.rotate_left((c.offset as i64).rem_euclid(len as i64) as usize);
    cw
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(start: Corner, direction: Direction) -> Config {
        Config { top: 49, right: 21, bottom: 48, left: 21, start, direction, ..Config::default() }
    }

    #[test]
    fn anticlockwise_from_bottom_right() {
        let r = build(&config(Corner::BottomRight, Direction::Anticlockwise));
        assert_eq!(r.len(), 139);
        // LED 1: bottom of the right side, LED 21: top of the right side.
        assert_eq!((r[0].side, r[0].y1), (Side::Right, 1.0));
        assert_eq!((r[20].side, r[20].y0), (Side::Right, 0.0));
        // Top runs right to left, left runs top to bottom, bottom runs left to right.
        assert_eq!((r[21].side, r[21].x1), (Side::Top, 1.0));
        assert_eq!((r[69].side, r[69].x0), (Side::Top, 0.0));
        assert_eq!((r[70].side, r[70].y0), (Side::Left, 0.0));
        assert_eq!((r[91].side, r[91].x0), (Side::Bottom, 0.0));
        assert_eq!((r[138].side, r[138].x1), (Side::Bottom, 1.0));
    }

    #[test]
    fn clockwise_from_top_left() {
        let r = build(&config(Corner::TopLeft, Direction::Clockwise));
        assert_eq!((r[0].side, r[0].x0), (Side::Top, 0.0));
        assert_eq!(r[49].side, Side::Right);
        assert_eq!((r[138].side, r[138].y0), (Side::Left, 0.0));
    }

    #[test]
    fn clockwise_from_bottom_left_goes_up_the_left_side() {
        let r = build(&config(Corner::BottomLeft, Direction::Clockwise));
        assert_eq!((r[0].side, r[0].y1), (Side::Left, 1.0));
        assert_eq!(r[21].side, Side::Top);
    }

    #[test]
    fn bottom_gap_leaves_the_middle_empty() {
        let c = Config { top: 0, right: 0, bottom: 4, left: 0, bottom_gap: 2, ..Config::default() };
        let mut xs: Vec<(f32, f32)> = build(&c).iter().map(|r| (r.x0, r.x1)).collect();
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let sixth = 1.0 / 6.0;
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(close(xs[1].1, 2.0 * sixth) && close(xs[2].0, 4.0 * sixth), "{xs:?}");
    }

    #[test]
    fn offset_moves_led_one_along_the_strip() {
        let base = build(&config(Corner::BottomRight, Direction::Anticlockwise));
        let shifted = build(&Config { offset: 3, ..config(Corner::BottomRight, Direction::Anticlockwise) });
        assert_eq!(shifted[0], base[3]);
        let back = build(&Config { offset: -1, ..config(Corner::BottomRight, Direction::Anticlockwise) });
        assert_eq!(back[0], base[base.len() - 1]);
    }

    #[test]
    fn edge_depth_sets_sampling_depth() {
        let r = build(&Config { edge_depth: 20.0, ..config(Corner::TopLeft, Direction::Clockwise) });
        assert!((r[0].y1 - 0.2).abs() < 1e-6);
    }

    #[test]
    fn empty_layout() {
        let c = Config { top: 0, right: 0, bottom: 0, left: 0, ..Config::default() };
        assert!(build(&c).is_empty());
    }
}
