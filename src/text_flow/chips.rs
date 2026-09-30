use super::geometry::*;
#[derive(Clone, Copy, Debug)]
pub struct ChipSize {
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChipPosition {
    pub x: f64,
    pub y: f64,
}
/// Packs indivisible boxes; callers constrain oversized contents before measuring.
pub fn pack_chips(
    width: f64,
    sizes: &[ChipSize],
    circles: &[Circle],
    gap: f64,
) -> (Vec<ChipPosition>, f64) {
    if width <= 0.0
        || !width.is_finite()
        || !gap.is_finite()
        || sizes.is_empty()
        || sizes.iter().any(|s| {
            !s.width.is_finite() || !s.height.is_finite() || s.width < 0.0 || s.height < 0.0
        })
    {
        return (Vec::new(), 0.0);
    }
    let gap = gap.max(0.0);
    let row = sizes.iter().map(|s| s.height).fold(1.0, f64::max);
    let widest = sizes.iter().map(|s| s.width.min(width)).fold(1.0, f64::max);
    let mut result = Vec::new();
    let mut y = 0.0;
    while result.len() < sizes.len() {
        for interval in free_intervals(width, circles, y, row, MIN_LINE_WIDTH.max(widest)) {
            let mut x = interval.x;
            while let Some(size) = sizes.get(result.len()) {
                let w = size.width.min(width);
                let actual = if x == interval.x { x } else { x + gap };
                if actual + w > interval.x + interval.width + 0.5 {
                    break;
                }
                result.push(ChipPosition { x: actual, y });
                x = actual + w;
            }
        }
        let next_y = y + row + gap;
        if !next_y.is_finite() || next_y <= y {
            return (Vec::new(), 0.0);
        }
        y = next_y;
    }
    (result, (y - gap).max(row))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_chip_progresses() {
        let (positions, h) = pack_chips(
            40.0,
            &[ChipSize {
                width: 200.0,
                height: 20.0,
            }],
            &[],
            0.0,
        );
        assert_eq!(positions.len(), 1);
        assert_eq!(h, 20.0);
    }
    #[test]
    fn zero_gap_email_stays_contiguous() {
        let (p, _) = pack_chips(
            100.0,
            &[ChipSize {
                width: 20.0,
                height: 20.0,
            }; 3],
            &[],
            0.0,
        );
        assert_eq!(
            p.iter().map(|p| p.x).collect::<Vec<_>>(),
            vec![0.0, 20.0, 40.0]
        );
    }
    #[test]
    fn blocked_rows_skip_below_obstacle() {
        let (p, _) = pack_chips(
            100.0,
            &[ChipSize {
                width: 80.0,
                height: 20.0,
            }],
            &[Circle {
                cx: 50.0,
                cy: 40.0,
                radius: 60.0,
            }],
            0.0,
        );
        assert!(p[0].y >= 100.0);
    }
    #[test]
    fn invalid_metrics_return_empty() {
        assert!(pack_chips(
            100.0,
            &[ChipSize {
                width: f64::NAN,
                height: 20.0
            }],
            &[],
            0.0
        )
        .0
        .is_empty());
    }
}
