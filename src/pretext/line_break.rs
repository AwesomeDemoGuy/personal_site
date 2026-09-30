use super::types::*;

fn validate(p: &PreparedText, c: LayoutCursor, width: f64) -> Result<()> {
    if width.is_nan() || width < 0.0 {
        return Err(LayoutError("width must be nonnegative and not NaN".into()));
    }
    if c.segment_index > p.measured.len()
        || (c.segment_index == p.measured.len() && c.grapheme_index != 0)
    {
        return Err(LayoutError("cursor is outside prepared text".into()));
    }
    if c.grapheme_index > 0
        && p.measured
            .get(c.segment_index)
            .and_then(|s| s.advances.as_ref())
            .is_none_or(|a| c.grapheme_index >= a.len())
    {
        return Err(LayoutError("cursor is outside segment graphemes".into()));
    }
    Ok(())
}
fn normalize(p: &PreparedText, c: &mut LayoutCursor) -> Option<usize> {
    let chunk_index = p
        .chunks
        .partition_point(|ch| c.segment_index >= ch.consumed_end);
    let chunk = p.chunks.get(chunk_index)?;
    if c.grapheme_index > 0 || (chunk.start == chunk.end && c.segment_index == chunk.start) {
        return Some(chunk_index);
    }
    c.segment_index = c.segment_index.max(chunk.start);
    while c.segment_index < chunk.end && p.measured[c.segment_index].kind.consumes_at_start() {
        c.segment_index += 1;
    }
    if c.segment_index < chunk.end {
        return Some(chunk_index);
    }
    if chunk.consumed_end >= p.measured.len() {
        return None;
    }
    c.segment_index = chunk.consumed_end;
    Some(chunk_index + 1)
}
fn terminal_spacing(p: &PreparedText, start: LayoutCursor, end: LayoutCursor) -> f64 {
    if p.letter_spacing == 0.0 {
        return 0.0;
    }
    if end.grapheme_index > 0 {
        return if p.measured[end.segment_index].spacing_count > 0 {
            p.letter_spacing
        } else {
            0.0
        };
    }
    for i in (start.segment_index..end.segment_index).rev() {
        let s = &p.measured[i];
        match s.kind {
            SegmentKind::Space | SegmentKind::ZeroWidthBreak | SegmentKind::HardBreak => continue,
            SegmentKind::SoftHyphen => {
                if i + 1 == end.segment_index {
                    return 0.0;
                }
                continue;
            }
            _ => {}
        }
        return if (i == start.segment_index && start.grapheme_index > 0) || s.spacing_count > 0 {
            p.letter_spacing
        } else {
            0.0
        };
    }
    0.0
}
fn finish(p: &PreparedText, start: LayoutCursor, end: LayoutCursor, width: f64) -> LineRange {
    LineRange {
        width: width + terminal_spacing(p, start, end),
        start,
        end,
    }
}
fn contribution(leading: f64, value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        leading + value
    }
}

fn simple_line(p: &PreparedText, start: LayoutCursor, max_width: f64) -> Option<LineRange> {
    let limit = max_width + p.profile.line_fit_epsilon;
    let mut width = 0.0;
    let mut content = false;
    let mut end = start;
    let mut pending: Option<(usize, f64)> = None;
    for i in start.segment_index..p.measured.len() {
        let s = &p.measured[i];
        if !content {
            let first = if i == start.segment_index {
                start.grapheme_index
            } else {
                0
            };
            if first > 0 || (s.width > limit && s.advances.is_some()) {
                let advances = s.advances.as_ref()?;
                let mut preferred = None;
                width = advances[first];
                content = true;
                end = LayoutCursor {
                    segment_index: i,
                    grapheme_index: first + 1,
                };
                if s.preferred.contains(&(first + 1)) {
                    preferred = Some((first + 1, width));
                }
                for (g, advance) in advances.iter().enumerate().skip(first + 1) {
                    if width + advance > limit {
                        if let Some((grapheme_index, width)) = preferred {
                            return Some(LineRange {
                                width,
                                start,
                                end: LayoutCursor {
                                    segment_index: i,
                                    grapheme_index,
                                },
                            });
                        }
                        return Some(LineRange { width, start, end });
                    }
                    width += advance;
                    end.grapheme_index = g + 1;
                    if s.preferred.contains(&(g + 1)) {
                        preferred = Some((g + 1, width));
                    }
                }
                end = LayoutCursor {
                    segment_index: i + 1,
                    grapheme_index: 0,
                };
            } else {
                width = s.width;
                content = true;
                end = LayoutCursor {
                    segment_index: i + 1,
                    grapheme_index: 0,
                };
            }
            if s.kind.breaks_after() {
                pending = Some((i + 1, width - s.width));
            }
            continue;
        }
        if width + s.width > limit {
            if s.kind.breaks_after() {
                return Some(LineRange {
                    width,
                    start,
                    end: LayoutCursor {
                        segment_index: i + 1,
                        grapheme_index: 0,
                    },
                });
            }
            if let Some((pi, pw)) = pending {
                if end
                    <= (LayoutCursor {
                        segment_index: pi,
                        grapheme_index: 0,
                    })
                {
                    return Some(LineRange {
                        width: pw,
                        start,
                        end: LayoutCursor {
                            segment_index: pi,
                            grapheme_index: 0,
                        },
                    });
                }
            }
            return Some(LineRange { width, start, end });
        }
        width += s.width;
        end = LayoutCursor {
            segment_index: i + 1,
            grapheme_index: 0,
        };
        if s.kind.breaks_after() {
            pending = Some((i + 1, width - s.width));
        }
    }
    content.then_some(LineRange { width, start, end })
}

/// Returns one line without text allocations or browser calls.
pub fn layout_next_line_range(
    p: &PreparedText,
    cursor: LayoutCursor,
    max_width: f64,
) -> Result<Option<LineRange>> {
    validate(p, cursor, max_width)?;
    let mut start = cursor;
    let Some(ci) = normalize(p, &mut start) else {
        return Ok(None);
    };
    if p.simple {
        return Ok(simple_line(p, start, max_width));
    }
    let chunk = p.chunks[ci];
    if chunk.start == chunk.end {
        return Ok(Some(LineRange {
            width: 0.0,
            start,
            end: LayoutCursor {
                segment_index: chunk.consumed_end,
                grapheme_index: 0,
            },
        }));
    }
    let limit = max_width + p.profile.line_fit_epsilon;
    let mut w = 0.0;
    let mut has_content = false;
    let mut end = start;
    let mut pending: Option<(usize, f64, f64, SegmentKind)> = None;
    for i in start.segment_index..chunk.end {
        let s = &p.measured[i];
        let g_start = if i == start.segment_index {
            start.grapheme_index
        } else {
            0
        };
        let leading = if has_content && s.spacing_count > 0 {
            p.letter_spacing
        } else {
            0.0
        };
        let sw = if s.kind == SegmentKind::Tab && p.tab_stop > 0.0 {
            let rem = (w + leading) % p.tab_stop;
            if rem.abs() <= 1e-6 {
                p.tab_stop
            } else {
                p.tab_stop - rem
            }
        } else {
            s.width
        };
        let advance = leading + sw;
        let fit = contribution(
            leading,
            if s.kind == SegmentKind::Tab {
                sw + if s.spacing_count > 0 {
                    p.letter_spacing
                } else {
                    0.0
                }
            } else {
                s.fit
            },
        );
        let break_fit = contribution(
            leading,
            if s.kind == SegmentKind::Tab {
                0.0
            } else {
                s.fit
            },
        );
        let break_paint = contribution(
            leading,
            if s.kind == SegmentKind::Tab {
                sw
            } else {
                s.paint
            },
        );
        if s.kind == SegmentKind::SoftHyphen && g_start == 0 {
            if has_content {
                end = LayoutCursor {
                    segment_index: i + 1,
                    grapheme_index: 0,
                };
                pending = Some((i + 1, w + p.hyphen_width, w + p.hyphen_width, s.kind));
            }
            continue;
        }
        if !has_content {
            if g_start > 0 || (fit > limit && s.advances.is_some()) {
                let advances = s
                    .advances
                    .as_ref()
                    .ok_or_else(|| LayoutError("segment is not breakable".into()))?;
                let mut preferred: Option<(usize, f64)> = None;
                for (g, gw) in advances.iter().enumerate().skip(g_start) {
                    if !has_content {
                        has_content = true;
                        w = *gw;
                    } else {
                        let candidate = w + gw + p.letter_spacing;
                        if candidate + p.letter_spacing > limit {
                            if let Some((ge, pw)) = preferred {
                                return Ok(Some(finish(
                                    p,
                                    start,
                                    LayoutCursor {
                                        segment_index: i,
                                        grapheme_index: ge,
                                    },
                                    pw,
                                )));
                            }
                            return Ok(Some(finish(p, start, end, w)));
                        }
                        w = candidate;
                    }
                    end = LayoutCursor {
                        segment_index: i,
                        grapheme_index: g + 1,
                    };
                    if s.preferred.contains(&(g + 1)) {
                        preferred = Some((g + 1, w));
                    }
                }
                end = LayoutCursor {
                    segment_index: i + 1,
                    grapheme_index: 0,
                };
            } else {
                has_content = true;
                w = sw;
                end = LayoutCursor {
                    segment_index: i + 1,
                    grapheme_index: 0,
                };
            }
            if s.kind.breaks_after() {
                pending = Some((
                    i + 1,
                    w - advance + break_fit,
                    w - advance + break_paint,
                    s.kind,
                ));
            }
            continue;
        }
        if w + fit > limit {
            // Incremental Pretext always takes a fitting discretionary hyphen.
            if let Some((pi, pf, pp, SegmentKind::SoftHyphen)) = pending {
                if pf <= limit {
                    return Ok(Some(finish(
                        p,
                        start,
                        LayoutCursor {
                            segment_index: pi,
                            grapheme_index: 0,
                        },
                        pp,
                    )));
                }
            }
            if s.kind.breaks_after() && w + break_fit <= limit {
                return Ok(Some(finish(
                    p,
                    start,
                    LayoutCursor {
                        segment_index: i + 1,
                        grapheme_index: 0,
                    },
                    w + break_paint,
                )));
            }
            if let Some((pi, pf, pp, _)) = pending {
                if pf <= limit
                    && end
                        <= (LayoutCursor {
                            segment_index: pi,
                            grapheme_index: 0,
                        })
                {
                    return Ok(Some(finish(
                        p,
                        start,
                        LayoutCursor {
                            segment_index: pi,
                            grapheme_index: 0,
                        },
                        pp,
                    )));
                }
            }
            let paint = match pending {
                Some((pi, _, pp, SegmentKind::SoftHyphen))
                    if pi == end.segment_index && end.grapheme_index == 0 =>
                {
                    pp
                }
                _ => w,
            };
            return Ok(Some(finish(p, start, end, paint)));
        }
        w += advance;
        end = LayoutCursor {
            segment_index: i + 1,
            grapheme_index: 0,
        };
        if s.kind.breaks_after() {
            pending = Some((
                i + 1,
                w - advance + break_fit,
                w - advance + break_paint,
                s.kind,
            ));
        }
    }
    if !has_content {
        return Ok(None);
    }
    let paint = match pending {
        Some((pi, _, pp, _)) if pi == chunk.consumed_end && end.grapheme_index == 0 => pp,
        _ => w,
    };
    Ok(Some(finish(
        p,
        start,
        LayoutCursor {
            segment_index: chunk.consumed_end,
            grapheme_index: 0,
        },
        paint,
    )))
}

pub fn walk_line_ranges(
    p: &PreparedText,
    width: f64,
    mut on_line: impl FnMut(LineRange),
) -> Result<usize> {
    let mut cursor = LayoutCursor::default();
    let mut count = 0;
    while let Some(line) = layout_next_line_range(p, cursor, width)? {
        if line.end <= cursor {
            return Err(LayoutError("layout did not advance".into()));
        }
        cursor = line.end;
        count += 1;
        on_line(line);
    }
    Ok(count)
}
pub fn measure_line_stats(p: &PreparedText, width: f64) -> Result<LineStats> {
    let mut max: f64 = 0.0;
    let count = walk_line_ranges(p, width, |l| max = max.max(l.width))?;
    Ok(LineStats {
        line_count: count,
        max_line_width: max,
    })
}
pub fn measure_natural_width(p: &PreparedText) -> Result<f64> {
    Ok(measure_line_stats(p, f64::INFINITY)?.max_line_width)
}
pub fn layout(p: &PreparedText, width: f64, line_height: f64) -> Result<Layout> {
    if !line_height.is_finite() || line_height <= 0.0 {
        return Err(LayoutError(
            "line height must be finite and positive".into(),
        ));
    }
    let stats = measure_line_stats(p, width)?;
    Ok(Layout {
        line_count: stats.line_count,
        height: stats.line_count as f64 * line_height,
    })
}
