use super::unicode::*;
use super::{data::*, types::*};

#[derive(Clone, Debug)]
pub(crate) struct Segment {
    pub text: String,
    pub word_like: bool,
    pub kind: SegmentKind,
    pub start: usize,
}
pub(crate) struct Analysis {
    pub normalized: String,
    pub segments: Vec<Segment>,
}
fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    let code = c as u32;
    let index = ranges.partition_point(|&(_, end)| end < code);
    ranges.get(index).is_some_and(|&(start, _)| start <= code)
}
pub(crate) fn mark(c: char) -> bool {
    in_ranges(c, MARK)
}
fn digit(c: char) -> bool {
    in_ranges(c, DECIMAL)
}
fn arabic(s: &str) -> bool {
    s.chars().any(|c| in_ranges(c, ARABIC))
}
pub(crate) fn emoji(s: &str) -> bool {
    s.chars().any(|c| in_ranges(c, EMOJI_PRESENTATION)) || s.contains('\u{FE0F}')
}
pub(crate) fn maybe_emoji(s: &str) -> bool {
    s.chars().any(|c| in_ranges(c, MAYBE_EMOJI))
}
pub(crate) fn cjk(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x4e00..=0x9fff | 0x3400..=0x4dbf | 131072..=173791 | 173824..=177983 | 177984..=178207 | 178208..=183983 | 183984..=191471 | 191472..=192093 | 194560..=195103 | 196608..=201551 | 201552..=205743 | 205744..=210041 | 0xf900..=0xfaff | 0x3000..=0x303f | 0x3040..=0x309f | 0x30a0..=0x30ff | 0x3130..=0x318f | 0xac00..=0xd7af | 0xff00..=0xffef))
}
fn affix(c: char) -> bool {
    NUMERIC_AFFIX
        .chunks_exact(2)
        .any(|r| (r[0]..=r[1]).contains(&(c as u32)))
}
fn last_significant(s: &str) -> Option<char> {
    s.chars().rev().find(|c| !mark(*c))
}
fn first_digit(s: &str) -> bool {
    s.chars().find(|c| !mark(*c)).is_some_and(digit)
}
pub(crate) fn numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| digit(c) || ":-/×,.+–—".contains(c))
}
fn has_digit(s: &str) -> bool {
    s.chars().any(digit)
}
fn escaped_quote(s: &str) -> bool {
    let mut saw = false;
    for c in s.chars() {
        if c == '\\' || mark(c) {
            continue;
        }
        if KINSOKU_END.contains(c) || LEFT_STICKY.contains(c) || "'’".contains(c) {
            saw = true;
        } else {
            return false;
        }
    }
    saw
}
fn sticky(s: &str) -> bool {
    if escaped_quote(s) {
        return true;
    }
    let mut saw = false;
    for c in s.chars() {
        if LEFT_STICKY.contains(c) || affix(c) {
            saw = true;
        } else if !saw || !mark(c) {
            return false;
        }
    }
    saw
}
fn forward(s: &str) -> bool {
    escaped_quote(s)
        || (!s.is_empty()
            && s.chars()
                .all(|c| KINSOKU_END.contains(c) || "'’".contains(c) || mark(c) || affix(c)))
}
pub(crate) fn closing_quote(s: &str) -> bool {
    for c in s.chars().rev() {
        if CLOSING_QUOTES.contains(c) {
            return true;
        }
        if !LEFT_STICKY.contains(c) {
            return false;
        }
    }
    false
}
pub(crate) fn keep_all_continue(s: &str, punctuation: bool) -> bool {
    let Some(last) = s.chars().last() else {
        return true;
    };
    if "\u{a0}\u{202f}\u{2060}\u{feff}".contains(last) {
        return false;
    }
    !punctuation
        || !(KINSOKU_START.contains(last) || LEFT_STICKY.contains(last) || "-‐–—".contains(last))
}
fn internal_symbol(c: char) -> bool {
    if c.is_ascii() {
        return matches!(c as u32, 33..=47 | 58..=64 | 91..=96 | 123..=126) && c != '-' && c != '?';
    }
    !"?֊-‐‒–—…‼‽⁉".contains(c)
        && !in_ranges(c, EMOJI_PRESENTATION)
        && in_ranges(c, PUNCT_SYMBOL_PRIVATE)
}
fn symbol_segment(s: &str) -> bool {
    let chars: Vec<_> = s.chars().filter(|c| !mark(*c)).collect();
    !chars.is_empty() && chars.into_iter().all(internal_symbol)
}
fn can_join(l: &Segment, r: &Segment) -> bool {
    let ls = !l.word_like && symbol_segment(&l.text);
    let rs = !r.word_like && symbol_segment(&r.text);
    let la = last_significant(&l.text).is_some_and(affix);
    let lj = (l.word_like || la)
        && last_significant(&l.text).is_some_and(|c| internal_symbol(c) || affix(c));
    (ls || rs || lj)
        && !cjk(&l.text)
        && !cjk(&r.text)
        && (l.word_like || ls || la)
        && (r.word_like || rs)
}
fn classify(c: char, ws: WhiteSpace) -> SegmentKind {
    match c {
        ' ' if ws == WhiteSpace::PreWrap => SegmentKind::PreservedSpace,
        '\t' if ws == WhiteSpace::PreWrap => SegmentKind::Tab,
        '\n' if ws == WhiteSpace::PreWrap => SegmentKind::HardBreak,
        ' ' => SegmentKind::Space,
        '\u{a0}' | '\u{202f}' | '\u{2060}' | '\u{feff}' => SegmentKind::Glue,
        '\u{200b}' => SegmentKind::ZeroWidthBreak,
        '\u{ad}' => SegmentKind::SoftHyphen,
        _ => SegmentKind::Text,
    }
}
pub(crate) fn normalize(text: &str, ws: WhiteSpace) -> String {
    if ws == WhiteSpace::PreWrap {
        return text.replace("\r\n", "\n").replace(['\r', '\u{c}'], "\n");
    }
    let mut result = String::new();
    let mut space = false;
    for c in text.chars() {
        if matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
            space = !result.is_empty();
        } else {
            if space {
                result.push(' ');
                space = false;
            }
            result.push(c);
        }
    }
    result
}
fn append(l: &mut Segment, r: &Segment) {
    l.text.push_str(&r.text);
    l.word_like |= r.word_like;
}
pub(crate) fn analyze<B: TextBackend>(
    backend: &mut B,
    text: &str,
    locale: Option<&str>,
    options: PrepareOptions,
    profile: EngineProfile,
) -> Result<Analysis> {
    let normalized = normalize(text, options.white_space);
    let words = backend.words(&normalized, locale)?;
    if words.iter().map(|w| w.text.as_str()).collect::<String>() != normalized {
        return Err(LayoutError(
            "segmenter did not preserve normalized source".into(),
        ));
    }
    let mut pieces: Vec<Segment> = Vec::new();
    let mut start = 0;
    for word in words {
        let mut local: Vec<Segment> = Vec::new();
        for (offset, c) in word.text.char_indices() {
            let kind = classify(c, options.white_space);
            let word_like = kind == SegmentKind::Text && word.word_like;
            if let Some(last) = local
                .last_mut()
                .filter(|s| s.kind == kind && s.word_like == word_like)
            {
                last.text.push(c);
            } else {
                local.push(Segment {
                    text: c.to_string(),
                    word_like,
                    kind,
                    start: start + offset,
                });
            }
        }
        pieces.extend(local);
        start += word.text.len();
    }
    let mut merged: Vec<Segment> = Vec::new();
    let mut repeats: Vec<Option<char>> = Vec::new();
    for piece in pieces {
        let is_text = piece.kind == SegmentKind::Text;
        let repeat = if is_text
            && !piece.word_like
            && piece.text.encode_utf16().count() == 1
            && piece.text != "-"
            && piece.text != "—"
        {
            piece.text.chars().next()
        } else {
            None
        };
        let join = merged.last().is_some_and(|prev| {
            if !is_text || prev.kind != SegmentKind::Text {
                return false;
            }
            let pc = cjk(&prev.text);
            (profile.carry_cjk_after_closing_quote
                && cjk(&piece.text)
                && pc
                && closing_quote(&prev.text))
                || (pc
                    && !piece.text.is_empty()
                    && piece
                        .text
                        .chars()
                        .all(|c| KINSOKU_START.contains(c) || LEFT_STICKY.contains(c)))
                || prev.text.ends_with('၏')
                || (piece.word_like
                    && arabic(&piece.text)
                    && arabic(&prev.text)
                    && prev.text.chars().last().is_some_and(|c| ":.،؛".contains(c)))
                || (repeat.is_some() && repeats.last().copied().flatten() == repeat)
                || (!piece.word_like
                    && !pc
                    && (sticky(&piece.text) || (piece.text == "-" && prev.word_like)))
        });
        if join {
            let same_repeat = repeat.is_some() && repeats.last().copied().flatten() == repeat;
            append(merged.last_mut().unwrap(), &piece);
            if !same_repeat {
                *repeats.last_mut().unwrap() = None;
            }
        } else {
            merged.push(piece);
            repeats.push(repeat);
        }
    }
    for i in 1..merged.len() {
        if merged[i].kind == SegmentKind::Text
            && !merged[i].word_like
            && escaped_quote(&merged[i].text)
            && merged[i - 1].kind == SegmentKind::Text
            && !cjk(&merged[i - 1].text)
        {
            let r = merged[i].clone();
            append(&mut merged[i - 1], &r);
            merged[i].text.clear();
        }
    }
    let mut prefixes = vec![String::new(); merged.len()];
    let mut next: Option<usize> = None;
    for i in (0..merged.len()).rev() {
        if merged[i].text.is_empty() {
            continue;
        }
        if let Some(n) = next {
            if merged[i].kind == SegmentKind::Text
                && !merged[i].word_like
                && merged[n].kind == SegmentKind::Text
                && (forward(&merged[i].text)
                    || (merged[i].text == "-" && first_digit(&merged[n].text)))
            {
                prefixes[n] = format!("{}{}", merged[i].text, prefixes[n]);
                merged[n].start = merged[i].start;
                merged[i].text.clear();
                continue;
            }
        }
        next = Some(i);
    }
    for (i, prefix) in prefixes.into_iter().enumerate() {
        if !prefix.is_empty() {
            merged[i].text = prefix + &merged[i].text;
        }
    }
    merged.retain(|s| !s.text.is_empty());
    let mut glued = Vec::new();
    let mut i = 0;
    while i < merged.len() {
        let mut s = merged[i].clone();
        i += 1;
        if s.kind == SegmentKind::Glue {
            while i < merged.len() && merged[i].kind == SegmentKind::Glue {
                append(&mut s, &merged[i]);
                i += 1;
            }
            if i < merged.len() && merged[i].kind == SegmentKind::Text {
                s.kind = SegmentKind::Text;
                append(&mut s, &merged[i]);
                i += 1;
            }
        }
        if s.kind == SegmentKind::Text {
            while i < merged.len() && merged[i].kind == SegmentKind::Glue {
                while i < merged.len() && merged[i].kind == SegmentKind::Glue {
                    append(&mut s, &merged[i]);
                    i += 1;
                }
                if i < merged.len() && merged[i].kind == SegmentKind::Text {
                    append(&mut s, &merged[i]);
                    i += 1;
                }
            }
        }
        glued.push(s);
    }
    merged = glued;
    // URL scheme/path grouping, followed by query grouping.
    let mut i = 0;
    while i < merged.len() {
        let t = &merged[i].text;
        let scheme = t.ends_with(':')
            && t.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && t[..t.len() - 1]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
        if merged[i].kind == SegmentKind::Text
            && (t.starts_with("www.")
                || (scheme
                    && merged
                        .get(i + 1)
                        .is_some_and(|s| s.kind == SegmentKind::Text && s.text == "//")))
        {
            let mut j = i + 1;
            while j < merged.len() && !merged[j].kind.boundary() {
                let r = merged[j].clone();
                let query = r.text.contains('?');
                append(&mut merged[i], &r);
                merged[i].word_like = true;
                merged[j].text.clear();
                j += 1;
                if query {
                    break;
                }
            }
        }
        i += 1;
    }
    merged.retain(|s| !s.text.is_empty());
    let mut query_merged = Vec::new();
    let mut i = 0;
    while i < merged.len() {
        let s = merged[i].clone();
        let query = s.text.contains('?') && (s.text.contains("://") || s.text.starts_with("www."));
        query_merged.push(s);
        i += 1;
        if query && i < merged.len() && !merged[i].kind.boundary() {
            let mut q = merged[i].clone();
            q.word_like = true;
            q.kind = SegmentKind::Text;
            i += 1;
            while i < merged.len() && !merged[i].kind.boundary() {
                append(&mut q, &merged[i]);
                i += 1;
            }
            query_merged.push(q);
        }
    }
    let mut nums = Vec::new();
    let mut i = 0;
    while i < query_merged.len() {
        let mut s = query_merged[i].clone();
        i += 1;
        if s.kind == SegmentKind::Text && numeric(&s.text) && has_digit(&s.text) {
            s.word_like = true;
            while i < query_merged.len()
                && query_merged[i].kind == SegmentKind::Text
                && numeric(&query_merged[i].text)
            {
                append(&mut s, &query_merged[i]);
                i += 1;
            }
        }
        nums.push(s);
    }
    let mut split_nums = Vec::new();
    for s in nums {
        let parts: Vec<_> = s.text.split('-').collect();
        if s.kind == SegmentKind::Text
            && parts.len() > 1
            && parts.iter().all(|s| numeric(s) && has_digit(s))
        {
            let mut offset = 0;
            for (j, part) in parts.iter().enumerate() {
                let text = if j + 1 < parts.len() {
                    format!("{part}-")
                } else {
                    part.to_string()
                };
                split_nums.push(Segment {
                    start: s.start + offset,
                    text: text.clone(),
                    word_like: true,
                    kind: SegmentKind::Text,
                });
                offset += text.len();
            }
        } else {
            split_nums.push(s);
        }
    }
    merged = Vec::new();
    let mut i = 0;
    while i < split_nums.len() {
        let mut s = split_nums[i].clone();
        i += 1;
        if s.kind == SegmentKind::Text {
            while i < split_nums.len()
                && split_nums[i].kind == SegmentKind::Text
                && can_join(&split_nums[i - 1], &split_nums[i])
            {
                append(&mut s, &split_nums[i]);
                i += 1;
            }
        }
        merged.push(s);
    }
    for i in 0..merged.len().saturating_sub(1) {
        if merged[i].kind == SegmentKind::Text
            && merged[i + 1].kind == SegmentKind::Text
            && cjk(&merged[i].text)
            && cjk(&merged[i + 1].text)
        {
            let mut split = merged[i].text.len();
            for (offset, c) in merged[i].text.char_indices().rev() {
                if mark(c) || KINSOKU_END.contains(c) || "'’".contains(c) {
                    split = offset;
                } else {
                    break;
                }
            }
            if split > 0 && split < merged[i].text.len() {
                let tail = merged[i].text.split_off(split);
                merged[i + 1].text = tail + &merged[i + 1].text;
                merged[i + 1].start = merged[i].start + split;
            }
        }
        if matches!(
            merged[i].kind,
            SegmentKind::Space | SegmentKind::PreservedSpace
        ) && merged[i].text.starts_with(' ')
            && merged[i].text[1..].chars().all(mark)
            && merged[i].text.len() > 1
            && merged[i + 1].kind == SegmentKind::Text
            && arabic(&merged[i + 1].text)
        {
            let marks = merged[i].text[1..].to_string();
            merged[i].text = " ".into();
            merged[i].word_like = false;
            merged[i + 1].text = marks + &merged[i + 1].text;
            merged[i + 1].start = merged[i].start + 1;
        }
    }
    if options.word_break == WordBreak::KeepAll {
        let mut kept = Vec::new();
        let mut i = 0;
        while i < merged.len() {
            let start = i;
            i += 1;
            if merged[start].kind == SegmentKind::Text {
                while i < merged.len()
                    && merged[i].kind == SegmentKind::Text
                    && keep_all_continue(
                        &merged[i - 1].text,
                        profile.break_keep_all_after_punctuation,
                    )
                {
                    i += 1;
                }
            }
            if merged[start..i].iter().any(|s| cjk(&s.text)) {
                let mut s = merged[start].clone();
                for r in &merged[start + 1..i] {
                    append(&mut s, r);
                }
                kept.push(s);
            } else {
                kept.extend_from_slice(&merged[start..i]);
            }
        }
        merged = kept;
    }
    Ok(Analysis {
        normalized,
        segments: merged,
    })
}
