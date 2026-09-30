//! Server-only Markdown rendering. Raw HTML is displayed as text and the final
//! fragment is sanitized before it can be passed to Leptos' `inner_html`.

use std::{borrow::Cow, collections::HashSet, sync::LazyLock};

use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

pub fn render(source: &str) -> String {
    let events: Vec<_> = Parser::new_ext(source, Options::ENABLE_STRIKETHROUGH)
        .map(|event| match event {
            Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
            other => other,
        })
        .collect();
    let events = add_heading_ids(apply_image_widths(events));
    let mut normalized = Vec::with_capacity(events.len());
    let mut index = 0;
    while index < events.len() {
        if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = &events[index] {
            let end = events[index + 1..]
                .iter()
                .position(|event| matches!(event, Event::End(TagEnd::CodeBlock)))
                .map(|offset| index + 1 + offset)
                .expect("Markdown code block events are balanced");
            let code: String = events[index + 1..end]
                .iter()
                .filter_map(|event| match event {
                    Event::Text(text) => Some(text.as_ref()),
                    _ => None,
                })
                .collect();
            if let Some(highlighted) = highlight_code(&code, info) {
                normalized.push(events[index].clone());
                normalized.push(Event::Html(highlighted.into()));
                normalized.push(events[end].clone());
                index = end + 1;
                continue;
            }
        }
        // Standalone images become figures. The browser adapter also separates
        // images mixed with prose so their neighboring text can flow around them.
        if matches!(events[index], Event::Start(Tag::Paragraph)) {
            let end = events[index + 1..]
                .iter()
                .position(|event| matches!(event, Event::End(TagEnd::Paragraph)))
                .map(|offset| index + 1 + offset)
                .expect("Markdown paragraph events are balanced");
            let inline = &events[index + 1..end];
            if is_image(inline) {
                let width = inline.iter().find_map(|event| match event {
                    Event::Html(html) => html.rsplit_once(" width=\"")
                        .and_then(|(_, tail)| tail.split_once('"'))
                        .and_then(|(value, _)| value.parse::<u32>().ok()),
                    _ => None,
                });
                let style = width.map(|width| format!(" style=\"--blog-image-width:{width}px\""))
                    .unwrap_or_default();
                normalized.push(Event::Html(format!("<figure class=\"blog-image\"{style}>").into()));
                normalized.extend(inline.iter().cloned());
                normalized.push(Event::Html("</figure>\n".into()));
                index = end + 1;
                continue;
            }
        }
        normalized.push(events[index].clone());
        index += 1;
    }
    let mut output = String::new();
    html::push_html(&mut output, normalized.into_iter().map(|event| {
        if let Event::Start(tag @ Tag::Link { .. }) = &event {
            if let Tag::Link { dest_url, .. } = tag {
                if dest_url.starts_with("/assets/") {
                    let mut opening = String::new();
                    html::push_html(&mut opening, std::iter::once(Event::Start(tag.clone())));
                    opening.pop(); // Replace the opening tag's final '>'.
                    opening.push_str(" download>");
                    return Event::Html(opening.into());
                }
            }
        }
        event
    }));

    // Escaping text does not validate Markdown link/image URL schemes.
    let mut sanitizer = ammonia::Builder::default();
    for heading in ["h1", "h2", "h3", "h4", "h5", "h6"] {
        sanitizer.add_tag_attributes(heading, &["id"]);
    }
    sanitizer
        .add_tag_attributes("figure", &["class", "style"])
        .add_tag_attributes("img", &["width", "style"])
        .add_tag_attributes("a", &["download"])
        .add_tag_attributes("code", &["class"])
        .add_tag_attributes("span", &["class"])
        .attribute_filter(|_, attribute, value| {
            if attribute == "style" {
                let width = value.strip_prefix("--blog-image-width:")
                    .and_then(|width| width.strip_suffix("px"))
                    .and_then(|width| width.parse::<u32>().ok());
                return width.filter(|width| *width > 0).map(|_| Cow::Borrowed(value));
            }
            Some(Cow::Borrowed(value))
        })
        .clean(&output)
        .to_string()
}

fn add_heading_ids<'a>(mut events: Vec<Event<'a>>) -> Vec<Event<'a>> {
    let mut used = HashSet::new();
    for index in 0..events.len() {
        if !matches!(events[index], Event::Start(Tag::Heading { .. })) { continue; }
        let mut title = String::new();
        for event in &events[index + 1..] {
            match event {
                Event::End(TagEnd::Heading(_)) => break,
                Event::Text(text) | Event::Code(text) => title.push_str(text),
                Event::SoftBreak | Event::HardBreak => title.push(' '),
                _ => {},
            }
        }
        let mut slug = String::new();
        for character in title.to_lowercase().chars() {
            if character.is_whitespace() {
                if !slug.is_empty() && !slug.ends_with('-') { slug.push('-'); }
            } else if character.is_alphanumeric() || matches!(character, '-' | '_') {
                slug.push(character);
            }
        }
        let base = slug.trim_matches('-');
        let base = if base.is_empty() { "section" } else { base };
        let mut id = base.to_owned();
        let mut suffix = 0;
        while !used.insert(id.clone()) {
            suffix += 1;
            id = format!("{base}-{suffix}");
        }
        if let Event::Start(Tag::Heading { id: heading_id, .. }) = &mut events[index] {
            *heading_id = Some(id.into());
        }
    }
    events
}

fn parse_image_width(text: &str) -> Option<(u32, usize)> {
    let trimmed = text.trim_start_matches([' ', '\t']);
    let value = trimmed.strip_prefix("{width=")?;
    let close = value.find('}')?;
    let number = value[..close].trim();
    let width = number.strip_suffix("px").unwrap_or(number).parse::<u32>().ok()?;
    if width == 0 { return None; }
    Some((width, text.len() - trimmed.len() + "{width=".len() + close + 1))
}

fn apply_image_widths<'a>(mut events: Vec<Event<'a>>) -> Vec<Event<'a>> {
    let mut output = Vec::with_capacity(events.len());
    let mut index = 0;
    while index < events.len() {
        if matches!(&events[index], Event::Text(text) if text.is_empty()) {
            index += 1;
            continue;
        }
        if matches!(events[index], Event::Start(Tag::Image { .. })) {
            let mut depth = 1;
            let mut end = index + 1;
            while depth > 0 {
                match &events[end] {
                    Event::Start(Tag::Image { .. }) => depth += 1,
                    Event::End(TagEnd::Image) => depth -= 1,
                    _ => {},
                }
                if depth > 0 { end += 1; }
            }
            let text_index = if matches!(events.get(end + 1), Some(Event::End(TagEnd::Link))) {
                end + 2
            } else { end + 1 };
            if let Some(Event::Text(text)) = events.get(text_index) {
                if let Some((width, consumed)) = parse_image_width(text) {
                    let remainder = text[consumed..].to_owned();
                    let mut image = String::new();
                    html::push_html(&mut image, events[index..=end].iter().cloned());
                    let image = image.strip_suffix(" />").expect("Markdown image HTML is self-closing");
                    output.push(Event::Html(format!("{image} width=\"{width}\" style=\"--blog-image-width:{width}px\" />").into()));
                    events[text_index] = Event::Text(if remainder.trim().is_empty() { String::new() } else { remainder }.into());
                    index = end + 1;
                    continue;
                }
            }
        }
        output.push(events[index].clone());
        index += 1;
    }
    output
}

fn highlight_code(code: &str, info: &str) -> Option<String> {
    let language = info.split_whitespace().next()?.to_ascii_lowercase();
    if matches!(language.as_str(), "text" | "plaintext" | "plain") {
        return None;
    }
    let syntax = SYNTAXES.find_syntax_by_token(&language)?;
    let mut generator = ClassedHTMLGenerator::new_with_class_style(
        syntax,
        &SYNTAXES,
        ClassStyle::SpacedPrefixed { prefix: "syntax-" },
    );
    for line in LinesWithEndings::from(code) {
        generator.parse_html_for_line_which_includes_newline(line).ok()?;
    }
    Some(generator.finalize())
}

fn is_image(events: &[Event<'_>]) -> bool {
    let events = if matches!(events.first(), Some(Event::Start(Tag::Link { .. })))
        && matches!(events.last(), Some(Event::End(TagEnd::Link)))
    {
        &events[1..events.len() - 1]
    } else {
        events
    };
    if let [Event::Html(html)] = events {
        return html.starts_with("<img ");
    }
    matches!(events.first(), Some(Event::Start(Tag::Image { .. })))
        && matches!(events.last(), Some(Event::End(TagEnd::Image)))
        && events
            .iter()
            .filter(|event| matches!(event, Event::Start(Tag::Image { .. })))
            .count()
            == 1
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn preserves_markdown_semantics_and_code_whitespace() {
        let html = render("## Heading\n\nA **bold** *italic* [link](/about) and `code`.  \nNext line.\n\n> Quote\n\n- Item\n\n```rust\n  let x = 1;\n\n  x\n```\n");
        for expected in [
            "<h2 id=\"heading\">Heading</h2>",
            "<strong>bold</strong>",
            "<em>italic</em>",
            "href=\"/about\"",
            "<code>code</code>",
            "<br>",
            "<blockquote>",
            "<li>",
            "class=\"language-rust\"",
        ] {
            assert!(html.contains(expected), "missing {expected}: {html}");
        }
        let text = ammonia::Builder::empty().clean(&html).to_string();
        assert!(text.contains("  let x = 1;\n\n  x\n"));
    }

    #[test]
    fn highlights_known_languages_and_preserves_escaped_code() {
        for language in ["python", "c", "rust", "javascript", "json", "bash"] {
            let html = render(&format!("```{language}\nvalue = \"<script>& 🦀\";\n\treturn 42;\n\n```\n"));
            assert!(html.contains("syntax-"), "missing highlighting for {language}");
            assert!(!html.contains("<script>"));
            let text = ammonia::Builder::empty().clean(&html).to_string();
            assert!(text.contains("value = \"&lt;script&gt;&amp; 🦀\";\n\treturn 42;\n\n"));
        }
    }

    #[test]
    fn unknown_and_unlabelled_code_stays_plain() {
        for language in ["", "text", "plaintext", "not-a-language"] {
            let html = render(&format!("```{language}\n  <tag> & code\n```\n"));
            assert!(!html.contains("syntax-"));
            assert!(html.contains("  &lt;tag&gt; &amp; code\n"));
        }
    }

    #[test]
    fn image_widths_work_for_standalone_linked_and_inline_images() {
        let html = render("![Photo](/photo.png){width=629}\n\n[![Linked](/linked.png)](/about){width=240px}\n\nBefore ![Inline](/inline.png){width=120} after.\n");
        assert_eq!(html.matches("<figure class=\"blog-image\"").count(), 2);
        for width in [629, 240, 120] {
            assert!(html.contains(&format!("width=\"{width}\"")));
            assert!(html.contains(&format!("--blog-image-width:{width}px")));
        }
        assert!(html.contains("href=\"/about\""));
        assert!(html.contains("Before <img"));
        assert!(html.contains(" after.</p>"));
        assert!(!html.contains("{width="));
    }

    #[test]
    fn invalid_image_widths_do_not_create_styles_and_code_is_unchanged() {
        for width in ["0", "-1", "25%", "1.5", "bad", "999999999999999999999", "2;position:fixed"] {
            let html = render(&format!("![Photo](/photo.png){{width={width}}}"));
            assert!(!html.contains("style="));
            assert!(html.contains("{width="));
        }
        let html = render("```text\n![Photo](/photo.png){width=629}\n```\n");
        assert!(!html.contains("<img"));
        assert!(html.contains("{width=629}"));
    }

    #[test]
    fn headings_have_unique_ids_that_match_chapter_links() {
        let html = render("[Chapter](#1-initial-observations)\n\n## 1. Initial *observations*\n\n## 1. Initial observations\n\n### `Café` & code!\n\n### 🦀\n");
        assert!(html.contains("href=\"#1-initial-observations\""));
        assert!(html.contains("id=\"1-initial-observations\""));
        assert!(html.contains("id=\"1-initial-observations-1\""));
        assert!(html.contains("id=\"café-code\""));
        assert!(html.contains("id=\"section\""));
    }

    #[test]
    fn local_asset_links_are_downloads_and_other_links_are_unchanged() {
        let html = render("[Binary](/assets/blog/post/binary)\n\n[Database](/assets/blog/post/data.i64)\n\n[Chapter](#chapter)\n\n[Page](/about)\n\n[External](https://example.com/file)");
        assert_eq!(html.matches(" download").count(), 2);
        for href in ["/assets/blog/post/binary", "/assets/blog/post/data.i64", "#chapter", "/about", "https://example.com/file"] {
            assert!(html.contains(&format!("href=\"{href}\"")));
        }
    }

    #[test]
    fn emits_figures_for_standalone_and_linked_images() {
        let html = render("![Diagram](/assets/diagram.png \"Diagram\")\n\n[![Photo](/assets/me.jpg)](/about)\n\nBefore ![inline](/assets/me.jpg) after.");
        assert_eq!(html.matches("<figure class=\"blog-image\">").count(), 2);
        assert!(html.contains("alt=\"Diagram\""));
        assert!(html.contains("<p>Before <img"));
        assert!(html.contains(" after.</p>"));
    }

    #[test]
    fn raw_html_is_text_and_unsafe_urls_are_removed() {
        let html = render("<script>alert('x')</script>\n\n<img src=x onerror=alert(1)>\n\n[bad](javascript:alert%281%29) ![bad](javascript:alert%281%29)\n\n[safe](https://example.com)");
        assert!(!html.contains("<script"));
        assert!(!html.contains("<img src=\"x\""));
        assert!(!html.contains("href=\"javascript:"));
        assert!(!html.contains("src=\"javascript:"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("href=\"https://example.com\""));
    }
}
