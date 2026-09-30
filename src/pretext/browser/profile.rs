use super::super::EngineProfile;
pub fn browser_profile() -> EngineProfile {
    let Some(window) = web_sys::window() else {
        return EngineProfile::default();
    };
    let nav = window.navigator();
    let ua = nav.user_agent().unwrap_or_default();
    let vendor = js_sys::Reflect::get(&nav, &"vendor".into())
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    let safari = vendor == "Apple Computer, Inc."
        && ua.contains("Safari/")
        && !["Chrome/", "Chromium/", "CriOS/", "FxiOS/", "EdgiOS/"]
            .iter()
            .any(|s| ua.contains(s));
    let chromium = ["Chrome/", "Chromium/", "CriOS/", "Edg/"]
        .iter()
        .any(|s| ua.contains(s));
    EngineProfile {
        line_fit_epsilon: if safari { 1.0 / 64.0 } else { 0.005 },
        carry_cjk_after_closing_quote: chromium,
        break_keep_all_after_punctuation: !safari,
        prefer_prefix_widths_for_breakable_runs: safari,
        prefer_early_soft_hyphen_break: safari,
    }
}
