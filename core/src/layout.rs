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

/// Fraction of the screen height sampled by top/bottom LEDs.
pub const TOP_BOTTOM_DEPTH: f32 = 0.08;
/// Fraction of the screen width sampled by left/right LEDs.
pub const SIDE_DEPTH: f32 = 0.05;

/// Regions in the order the LEDs are wired.
pub fn build(c: &Config) -> Vec<Region> {
    let span = |i: u16, n: u16| (i as f32 / n as f32, (i + 1) as f32 / n as f32);

    // Clockwise from the top-left corner: top L→R, right T→B, bottom R→L, left B→T.
    let mut cw = Vec::with_capacity(c.led_count());
    for i in 0..c.top {
        let (s, e) = span(i, c.top);
        cw.push(Region { side: Side::Top, x0: s, y0: 0.0, x1: e, y1: TOP_BOTTOM_DEPTH });
    }
    for i in 0..c.right {
        let (s, e) = span(i, c.right);
        cw.push(Region { side: Side::Right, x0: 1.0 - SIDE_DEPTH, y0: s, x1: 1.0, y1: e });
    }
    for i in (0..c.bottom).rev() {
        let (s, e) = span(i, c.bottom);
        cw.push(Region { side: Side::Bottom, x0: s, y0: 1.0 - TOP_BOTTOM_DEPTH, x1: e, y1: 1.0 });
    }
    for i in (0..c.left).rev() {
        let (s, e) = span(i, c.left);
        cw.push(Region { side: Side::Left, x0: 0.0, y0: s, x1: SIDE_DEPTH, y1: e });
    }
    if cw.is_empty() {
        return cw;
    }

    let offset = match c.start {
        Corner::TopLeft => 0,
        Corner::TopRight => c.top as usize,
        Corner::BottomRight => (c.top + c.right) as usize,
        Corner::BottomLeft => (c.top + c.right + c.bottom) as usize,
    };
    let len = cw.len();
    cw.rotate_left(offset % len);
    // Going anticlockwise from a corner is the clockwise run that ends there, reversed.
    if c.direction == Direction::Anticlockwise {
        cw.reverse();
    }
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
    fn empty_layout() {
        let c = Config { top: 0, right: 0, bottom: 0, left: 0, ..Config::default() };
        assert!(build(&c).is_empty());
    }
}
