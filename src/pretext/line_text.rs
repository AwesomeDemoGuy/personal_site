use super::{line_break::*, types::*};

pub fn materialize_line_range(
    p: &PreparedTextWithSegments,
    range: &LineRange,
) -> Result<LayoutLine> {
    if range.start > range.end || range.end.segment_index > p.segments.len() {
        return Err(LayoutError("invalid line range".into()));
    }
    let mut text = String::new();
    for i in range.start.segment_index..range.end.segment_index {
        if matches!(p.kinds[i], SegmentKind::SoftHyphen | SegmentKind::HardBreak) {
            continue;
        }
        if i == range.start.segment_index && range.start.grapheme_index > 0 {
            let gs = p.graphemes[i]
                .get(range.start.grapheme_index..)
                .ok_or_else(|| LayoutError("invalid grapheme range".into()))?;
            for g in gs {
                text.push_str(g);
            }
        } else {
            text.push_str(&p.segments[i]);
        }
    }
    let hyphen = range.end.segment_index > range.start.segment_index
        && p.kinds[range.end.segment_index - 1] == SegmentKind::SoftHyphen;
    if hyphen {
        text.push('-');
    }
    if range.end.grapheme_index > 0 {
        let begin = if range.start.segment_index == range.end.segment_index {
            range.start.grapheme_index
        } else {
            0
        };
        let gs = p
            .graphemes
            .get(range.end.segment_index)
            .and_then(|gs| gs.get(begin..range.end.grapheme_index))
            .ok_or_else(|| LayoutError("invalid grapheme range".into()))?;
        for g in gs {
            text.push_str(g);
        }
    }
    Ok(LayoutLine {
        text,
        width: range.width,
        start: range.start,
        end: range.end,
    })
}
pub fn layout_next_line(
    p: &PreparedTextWithSegments,
    cursor: LayoutCursor,
    width: f64,
) -> Result<Option<LayoutLine>> {
    layout_next_line_range(&p.geometry, cursor, width)?
        .map(|r| materialize_line_range(p, &r))
        .transpose()
}
pub fn layout_with_lines(
    p: &PreparedTextWithSegments,
    width: f64,
    line_height: f64,
) -> Result<LayoutWithLines> {
    let geometry = layout(&p.geometry, width, line_height)?;
    let mut lines = Vec::with_capacity(geometry.line_count);
    let mut cursor = LayoutCursor::default();
    while let Some(line) = layout_next_line(p, cursor, width)? {
        cursor = line.end;
        lines.push(line);
    }
    Ok(LayoutWithLines {
        line_count: geometry.line_count,
        height: geometry.height,
        lines,
    })
}
