#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub x: f64,
    pub width: f64,
}
pub const PHOTO_MARGIN: f64 = 18.0;
pub const MIN_LINE_WIDTH: f64 = 64.0;
/// Free intervals in reading order. A fully occluded row has no intervals.
pub fn free_intervals(
    width: f64,
    circles: &[Circle],
    y: f64,
    height: f64,
    min_gap: f64,
) -> Vec<Interval> {
    if !width.is_finite() || width <= 0.0 || !y.is_finite() || !height.is_finite() || height <= 0.0
    {
        return Vec::new();
    }
    let mut excluded = Vec::new();
    for c in circles {
        if !c.cx.is_finite() || !c.cy.is_finite() || !c.radius.is_finite() || c.radius <= 0.0 {
            continue;
        }
        let r = c.radius + PHOTO_MARGIN;
        let dy = (y - c.cy).max(c.cy - (y + height)).max(0.0);
        if dy >= r {
            continue;
        }
        let dx = (r * r - dy * dy).sqrt();
        let left = (c.cx - dx).max(0.0);
        let right = (c.cx + dx).min(width);
        if right > left {
            excluded.push((left, right));
        }
    }
    if excluded.is_empty() {
        return vec![Interval { x: 0.0, width }];
    }
    excluded.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (l, r) in excluded {
        if let Some(last) = merged.last_mut().filter(|last| l <= last.1) {
            last.1 = last.1.max(r);
        } else {
            merged.push((l, r));
        }
    }
    let mut result = Vec::new();
    let mut cursor = 0.0;
    for (l, r) in merged {
        if l - cursor >= min_gap {
            result.push(Interval {
                x: cursor,
                width: l - cursor,
            });
        }
        cursor = cursor.max(r);
    }
    if width - cursor >= min_gap {
        result.push(Interval {
            x: cursor,
            width: width - cursor,
        });
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocked_rows_are_empty() {
        assert!(free_intervals(
            80.0,
            &[Circle {
                cx: 40.0,
                cy: 10.0,
                radius: 50.0
            }],
            0.0,
            20.0,
            10.0
        )
        .is_empty());
    }
    #[test]
    fn overlapping_circles_merge() {
        let xs = free_intervals(
            400.0,
            &[
                Circle {
                    cx: 150.0,
                    cy: 10.0,
                    radius: 40.0,
                },
                Circle {
                    cx: 200.0,
                    cy: 10.0,
                    radius: 40.0,
                },
            ],
            0.0,
            20.0,
            20.0,
        );
        assert_eq!(
            xs,
            vec![
                Interval {
                    x: 0.0,
                    width: 92.0
                },
                Interval {
                    x: 258.0,
                    width: 142.0
                }
            ]
        );
    }
    #[test]
    fn narrow_unobstructed_columns_still_progress() {
        assert_eq!(
            free_intervals(20.0, &[], 0.0, 24.0, 64.0),
            vec![Interval {
                x: 0.0,
                width: 20.0
            }]
        );
    }
}
