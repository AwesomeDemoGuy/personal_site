//! Compatibility metadata, not a full Unicode bidi shaping/reordering engine.
use super::data::*;
fn classify(c: char) -> &'static str {
    let cp = c as u32;
    if cp < 256 {
        return LATIN1[cp as usize];
    }
    let index = BIDI_RANGES.partition_point(|r| r.1 < cp);
    BIDI_RANGES
        .get(index)
        .filter(|r| r.0 <= cp)
        .map_or("L", |r| r.2)
}
pub(crate) fn segment_levels(text: &str, starts: &[usize]) -> Option<Vec<i8>> {
    // Keep UTF-16 duplication for supplementary code points, matching the oracle.
    let mut types: Vec<_> = text
        .chars()
        .flat_map(|c| std::iter::repeat_n(classify(c), c.len_utf16()))
        .collect();
    if !types.iter().any(|t| matches!(*t, "R" | "AL" | "AN")) {
        return None;
    }
    let start = types
        .iter()
        .find(|t| matches!(**t, "L" | "R" | "AL"))
        .map_or(0, |t| if *t == "L" { 0 } else { 1 });
    let sor = if start == 0 { "L" } else { "R" };
    let mut last = sor;
    for t in &mut types {
        if *t == "NSM" {
            *t = last;
        } else {
            last = *t;
        }
    }
    last = sor;
    for t in &mut types {
        if *t == "EN" {
            if last == "AL" {
                *t = "AN";
            }
        } else if matches!(*t, "R" | "L" | "AL") {
            last = *t;
        }
    }
    for t in &mut types {
        if *t == "AL" {
            *t = "R";
        }
    }
    for i in 1..types.len().saturating_sub(1) {
        if types[i] == "ES" && types[i - 1] == "EN" && types[i + 1] == "EN" {
            types[i] = "EN";
        }
        if types[i] == "CS" && matches!(types[i - 1], "EN" | "AN") && types[i + 1] == types[i - 1] {
            types[i] = types[i - 1];
        }
    }
    for i in 0..types.len() {
        if types[i] != "EN" {
            continue;
        }
        let mut j = i;
        while j > 0 && types[j - 1] == "ET" {
            j -= 1;
            types[j] = "EN";
        }
        let mut j = i + 1;
        while j < types.len() && types[j] == "ET" {
            types[j] = "EN";
            j += 1;
        }
    }
    for t in &mut types {
        if matches!(*t, "WS" | "ES" | "ET" | "CS") {
            *t = "ON";
        }
    }
    last = sor;
    for t in &mut types {
        if *t == "EN" {
            if last == "L" {
                *t = "L";
            }
        } else if matches!(*t, "R" | "L") {
            last = *t;
        }
    }
    let mut i = 0;
    while i < types.len() {
        if types[i] != "ON" {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < types.len() && types[end] == "ON" {
            end += 1;
        }
        let before = if i > 0 { types[i - 1] } else { sor };
        let after = types.get(end).copied().unwrap_or(sor);
        let b = if before == "L" { "L" } else { "R" };
        let a = if after == "L" { "L" } else { "R" };
        for t in &mut types[i..end] {
            *t = if b == a { b } else { sor };
        }
        i = end;
    }
    let levels: Vec<i8> = types
        .into_iter()
        .map(|t| {
            start
                + if start == 0 {
                    match t {
                        "R" => 1,
                        "AN" | "EN" => 2,
                        _ => 0,
                    }
                } else {
                    i8::from(matches!(t, "L" | "AN" | "EN"))
                }
        })
        .collect();
    Some(
        starts
            .iter()
            .map(|s| levels[text[..*s].encode_utf16().count()])
            .collect(),
    )
}
