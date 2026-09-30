use super::{analysis::*, bidi, data::*, types::*};
use std::collections::HashMap;

/// Owned preparation services and bounded caches; prepared handles are immutable.
pub struct TextEngine<B> {
    backend: B,
    profile: EngineProfile,
    locale: Option<String>,
    metrics: HashMap<(String, String), f64>,
    corrections: HashMap<String, f64>,
}
impl<B: TextBackend> TextEngine<B> {
    pub fn new(backend: B, profile: EngineProfile) -> Self {
        Self {
            backend,
            profile,
            locale: None,
            metrics: HashMap::new(),
            corrections: HashMap::new(),
        }
    }
    pub fn clear_cache(&mut self) {
        self.metrics.clear();
        self.corrections.clear();
        self.backend.clear_cache();
    }
    pub fn set_locale(&mut self, locale: Option<&str>) {
        self.locale = locale.filter(|s| !s.is_empty()).map(str::to_owned);
        self.clear_cache();
    }
    pub fn prepare(
        &mut self,
        text: &str,
        font: &str,
        options: PrepareOptions,
    ) -> Result<PreparedText> {
        Ok(self.prepare_with_segments(text, font, options)?.geometry)
    }
    fn width(&mut self, font: &str, text: &str, correction: f64) -> Result<f64> {
        let key = (font.to_owned(), text.to_owned());
        let width = if let Some(w) = self.metrics.get(&key) {
            *w
        } else {
            let w = self.backend.measure(font, text)?;
            if !w.is_finite() {
                return Err(LayoutError(
                    "measurement returned a non-finite width".into(),
                ));
            }
            if self.metrics.len() >= 8192 {
                self.metrics.clear();
            }
            self.metrics.insert(key, w);
            w
        };
        if correction == 0.0 {
            return Ok(width);
        }
        let count = self
            .backend
            .graphemes(text)?
            .iter()
            .filter(|g| emoji(g))
            .count();
        Ok(width - count as f64 * correction)
    }
    fn cjk_units(&mut self, text: &str, keep_all: bool) -> Result<Vec<(String, usize)>> {
        let mut units: Vec<(String, usize)> = Vec::new();
        let mut offset = 0;
        let mut contains_cjk = false;
        let mut ends_quote = false;
        let mut single_open = false;
        for g in self.backend.graphemes(text)? {
            let gc = cjk(&g);
            let one = g.chars().next().filter(|_| g.encode_utf16().count() == 1);
            let join = !units.is_empty()
                && (single_open
                    || one.is_some_and(|c| KINSOKU_START.contains(c) || LEFT_STICKY.contains(c))
                    || (self.profile.carry_cjk_after_closing_quote && gc && ends_quote)
                    || (!contains_cjk && !gc));
            if join {
                units.last_mut().unwrap().0.push_str(&g);
                contains_cjk |= gc;
                if one.is_some_and(|c| LEFT_STICKY.contains(c)) {
                    ends_quote |= closing_quote(&g);
                } else {
                    ends_quote = closing_quote(&g);
                }
                single_open = false;
            } else {
                units.push((g.clone(), offset));
                contains_cjk = gc;
                ends_quote = closing_quote(&g);
                single_open = one.is_some_and(|c| KINSOKU_END.contains(c));
            }
            offset += g.len();
        }
        if !keep_all || units.len() <= 1 {
            return Ok(units);
        }
        let mut kept = Vec::new();
        let mut i = 0;
        while i < units.len() {
            let start = i;
            i += 1;
            while i < units.len()
                && keep_all_continue(
                    &units[i - 1].0,
                    self.profile.break_keep_all_after_punctuation,
                )
            {
                i += 1;
            }
            if units[start..i].iter().any(|u| cjk(&u.0)) {
                kept.push((
                    units[start..i].iter().map(|u| u.0.as_str()).collect(),
                    units[start].1,
                ));
            } else {
                kept.extend_from_slice(&units[start..i]);
            }
        }
        Ok(kept)
    }
    pub fn prepare_with_segments(
        &mut self,
        text: &str,
        font: &str,
        options: PrepareOptions,
    ) -> Result<PreparedTextWithSegments> {
        if !options.letter_spacing.is_finite() {
            return Err(LayoutError("letter spacing must be finite".into()));
        }
        let analysis = analyze(
            &mut self.backend,
            text,
            self.locale.as_deref(),
            options,
            self.profile,
        )?;
        let mut correction = 0.0;
        if maybe_emoji(&analysis.normalized) {
            correction = if let Some(c) = self.corrections.get(font) {
                *c
            } else {
                let c = self.backend.emoji_correction(font)?;
                self.corrections.insert(font.into(), c);
                c
            };
        }
        let spacing = options.letter_spacing;
        let hyphen =
            self.width(font, "-", correction)? + if spacing == 0.0 { 0.0 } else { spacing * 2.0 };
        let tab = self.width(font, " ", correction)? * 8.0;
        let mut measured = Vec::new();
        let mut segments = Vec::new();
        let mut kinds = Vec::new();
        let mut graphemes = Vec::new();
        let mut starts = Vec::new();
        for s in analysis.segments {
            let units = if s.kind == SegmentKind::Text && cjk(&s.text) {
                self.cjk_units(&s.text, options.word_break == WordBreak::KeepAll)?
            } else {
                vec![(s.text.clone(), 0)]
            };
            for (unit, offset) in units {
                let gs = self.backend.graphemes(&unit)?;
                let count = if spacing == 0.0
                    || matches!(
                        s.kind,
                        SegmentKind::ZeroWidthBreak
                            | SegmentKind::SoftHyphen
                            | SegmentKind::HardBreak
                    ) {
                    0
                } else if s.kind == SegmentKind::Tab {
                    1
                } else {
                    gs.len()
                };
                let mut m = MeasuredSegment {
                    width: 0.0,
                    fit: 0.0,
                    paint: 0.0,
                    kind: s.kind,
                    advances: None,
                    preferred: Vec::new(),
                    spacing_count: count,
                };
                match s.kind {
                    SegmentKind::SoftHyphen => {
                        m.fit = hyphen;
                        m.paint = hyphen;
                    }
                    SegmentKind::HardBreak | SegmentKind::Tab => {}
                    _ => {
                        m.width = self.width(font, &unit, correction)?
                            + count.saturating_sub(1) as f64 * spacing;
                        let no_fit = matches!(
                            s.kind,
                            SegmentKind::Space
                                | SegmentKind::PreservedSpace
                                | SegmentKind::ZeroWidthBreak
                        );
                        m.fit = if no_fit || m.width == 0.0 {
                            0.0
                        } else {
                            m.width + if count > 0 { spacing } else { 0.0 }
                        };
                        m.paint =
                            if matches!(s.kind, SegmentKind::Space | SegmentKind::ZeroWidthBreak) {
                                0.0
                            } else {
                                m.width
                            };
                        let allow = s.kind != SegmentKind::Text
                            || !cjk(&s.text)
                            || options.word_break == WordBreak::KeepAll
                            || !cjk(&unit);
                        if allow && s.word_like && unit.encode_utf16().count() > 1 && gs.len() > 1 {
                            let prefix = spacing != 0.0
                                || (!numeric(&unit)
                                    && self.profile.prefer_prefix_widths_for_breakable_runs);
                            let pair = (!prefix && numeric(&unit)) || (prefix && gs.len() > 96);
                            let mut advances = Vec::new();
                            let mut previous = String::new();
                            let mut previous_width = 0.0;
                            for g in &gs {
                                let gw = if prefix && !pair {
                                    0.0
                                } else {
                                    self.width(font, g, correction)?
                                };
                                let advance = if pair && !previous.is_empty() {
                                    self.width(font, &(previous.clone() + g), correction)?
                                        - previous_width
                                } else if prefix && !pair {
                                    let next =
                                        self.width(font, &(previous.clone() + g), correction)?;
                                    let diff = next - previous_width;
                                    previous_width = next;
                                    diff
                                } else {
                                    gw
                                };
                                advances.push(advance);
                                if pair {
                                    previous = g.clone();
                                    previous_width = gw;
                                } else if prefix {
                                    previous.push_str(g);
                                }
                            }
                            if options.word_break != WordBreak::KeepAll {
                                m.preferred = gs
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, g)| {
                                        matches!(g.as_str(), "-" | "֊" | "‐" | "‒" | "–" | "—")
                                    })
                                    .map(|(i, _)| i + 1)
                                    .collect();
                            }
                            m.advances = Some(advances);
                        }
                    }
                }
                measured.push(m);
                starts.push(s.start + offset);
                kinds.push(s.kind);
                segments.push(unit);
                graphemes.push(gs);
            }
        }
        let mut chunks = Vec::new();
        let mut start = 0;
        for (i, s) in measured.iter().enumerate() {
            if s.kind == SegmentKind::HardBreak {
                chunks.push(Chunk {
                    start,
                    end: i,
                    consumed_end: i + 1,
                });
                start = i + 1;
            }
        }
        if start < measured.len() {
            chunks.push(Chunk {
                start,
                end: measured.len(),
                consumed_end: measured.len(),
            });
        }
        let simple = chunks.len() <= 1
            && spacing == 0.0
            && measured.iter().all(|s| {
                matches!(
                    s.kind,
                    SegmentKind::Text | SegmentKind::Space | SegmentKind::ZeroWidthBreak
                )
            });
        let segment_levels = bidi::segment_levels(&analysis.normalized, &starts);
        let empty = measured.is_empty();
        Ok(PreparedTextWithSegments {
            geometry: PreparedText {
                measured,
                chunks,
                letter_spacing: if empty { 0.0 } else { spacing },
                hyphen_width: if empty { 0.0 } else { hyphen },
                tab_stop: if empty { 0.0 } else { tab },
                profile: self.profile,
                simple,
            },
            segments,
            kinds,
            graphemes,
            segment_levels,
        })
    }
}
