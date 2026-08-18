//! metruyenhot chapter-page parsing: CSS selectors, the JS-hidden noise
//! filter, and the obfuscated `contentS` splice. Nothing here escapes the
//! adapter except [`extract_full_chapter_text`].

use std::collections::HashSet;

use anyhow::{Result, anyhow};
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};

use crate::source::ChapterContent;
use crate::utils::{clean_text, is_noise};

const NON_CONTENT_ATTRS: &[&str] = &[
    "class",
    "style",
    "id",
    "onmousedown",
    "onselectstart",
    "oncopy",
    "oncut",
];

/// Pre-compiled regex pulling the obfuscated `contentS` JS string from a
/// script block. `(?s)` enables single-line mode so `.` matches newlines.
static CONTENT_S_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?s)var\s+contentS\s*=\s*'(.*?)';\s*div\.innerHTML"#).unwrap());

/// Extract all text inside an element (cheerio's `.text()`), then [`clean_text`].
fn element_text(elem: &ElementRef<'_>) -> String {
    let combined: String = elem.text().collect();
    clean_text(&combined)
}

/// Matches one CSS rule: a selector list, then a `{ ... }` declaration block.
/// Both groups exclude braces so the match lands on the innermost rule, which
/// keeps it working for rules nested inside at-rules (e.g. `@media`).
static CSS_RULE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)([^{}]*)\{([^{}]*)\}").unwrap());

/// Matches a `.class-name` token inside a CSS selector.
static CSS_CLASS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\.([A-Za-z0-9_-]+)").unwrap());

/// Matches a `display: none` declaration (whitespace-tolerant).
static DISPLAY_NONE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)display\s*:\s*none").unwrap());

/// Scan every `<style>` block in `doc` and collect the set of class names that
/// are hidden via a `display: none` rule. metruyenhot hides its injected
/// promo/noise paragraphs this way under a per-page rotating class name
/// (`mshow-hb`, `mshow-bs`, `ms-b`, ...), so detecting the hidden classes
/// dynamically is more robust than hard-coding the rotating names.
fn hidden_class_names(doc: &Html) -> HashSet<String> {
    let style_sel = Selector::parse("style").unwrap();
    let mut hidden = HashSet::new();
    for style in doc.select(&style_sel) {
        let css: String = style.text().collect();
        for rule in CSS_RULE_RE.captures_iter(&css) {
            let selectors = &rule[1];
            let declarations = &rule[2];
            if !DISPLAY_NONE_RE.is_match(declarations) {
                continue;
            }
            for class in CSS_CLASS_RE.captures_iter(selectors) {
                hidden.insert(class[1].to_string());
            }
        }
    }
    hidden
}

/// True when `elem` carries a class listed in `hidden`, marking it as a
/// CSS-hidden noise element to skip.
fn is_hidden_noise_element(elem: &ElementRef<'_>, hidden: &HashSet<String>) -> bool {
    if hidden.is_empty() {
        return false;
    }
    elem.value()
        .attr("class")
        .map(|class| class.split_whitespace().any(|token| hidden.contains(token)))
        .unwrap_or(false)
}

/// Try to derive a non-empty text representation for `elem`. Falls back to
/// the first attribute value (excluding presentation/scripting attributes)
/// when the inner text is empty, mirroring the TS extractor.
fn extract_text_from_element(elem: &ElementRef<'_>) -> Option<String> {
    let normal = element_text(elem);
    if !normal.is_empty() {
        return Some(normal);
    }
    for attr in elem.value().attrs() {
        let (name, value) = attr;
        if NON_CONTENT_ATTRS
            .iter()
            .any(|n| n.eq_ignore_ascii_case(name))
        {
            continue;
        }
        let candidate = clean_text(value);
        if !candidate.is_empty() {
            return Some(candidate);
        }
    }
    None
}

/// Pull the obfuscated injected paragraphs out of the page's inline JS, parse
/// them as HTML, and return cleaned non-noise paragraph texts. Returns an
/// empty vector if no `contentS` block is found.
fn extract_injected_content_from_script(full_html: &str, hidden: &HashSet<String>) -> Vec<String> {
    let captures = match CONTENT_S_RE.captures(full_html) {
        Some(c) => c,
        None => return Vec::new(),
    };
    let raw = captures.get(1).map(|m| m.as_str()).unwrap_or("");
    let js_html = raw.replace("\\'", "'").replace("\\\"", "\"");

    let doc = Html::parse_fragment(&js_html);
    let p_sel = Selector::parse("p").unwrap();
    let mut out = Vec::new();
    for p in doc.select(&p_sel) {
        if is_hidden_noise_element(&p, hidden) {
            continue;
        }
        if let Some(text) = extract_text_from_element(&p)
            && !is_noise(&text)
        {
            out.push(text);
        }
    }
    out
}

/// Pick the novel title from `.rv-full-story-title h1`, then the first
/// non-empty `<h1>`, defaulting to "Unknown Novel".
fn extract_novel_title(doc: &Html) -> String {
    let primary = Selector::parse(".rv-full-story-title h1").unwrap();
    if let Some(elem) = doc.select(&primary).next() {
        let text = element_text(&elem);
        if !text.is_empty() {
            return text;
        }
    }
    let h1_sel = Selector::parse("h1").unwrap();
    for elem in doc.select(&h1_sel) {
        let text = element_text(&elem);
        if !text.is_empty() {
            return text;
        }
    }
    "Unknown Novel".to_string()
}

/// Pick the chapter title from `.rv-chapt-title h2`, then the first non-empty
/// `<h1>` or `<h2>`, defaulting to "Untitled Chapter".
fn extract_chapter_title(doc: &Html) -> String {
    let primary = Selector::parse(".rv-chapt-title h2").unwrap();
    if let Some(elem) = doc.select(&primary).next() {
        let text = element_text(&elem);
        if !text.is_empty() {
            return text;
        }
    }
    let fallback = Selector::parse("h1, h2").unwrap();
    for elem in doc.select(&fallback) {
        let text = element_text(&elem);
        if !text.is_empty() {
            return text;
        }
    }
    "Untitled Chapter".to_string()
}

/// Parse a fetched chapter page and return its title, novel title, and
/// cleaned paragraphs. Errors when the `.chapter-c` container is missing.
pub fn extract_full_chapter_text(full_html: &str) -> Result<ChapterContent> {
    let doc = Html::parse_document(full_html);
    let chapter_sel = Selector::parse(".chapter-c").unwrap();
    let chapter = doc
        .select(&chapter_sel)
        .next()
        .ok_or_else(|| anyhow!("Could not find .chapter-c in the HTML"))?;

    let novel_title = extract_novel_title(&doc);
    let chapter_title = extract_chapter_title(&doc);
    let hidden = hidden_class_names(&doc);
    let injected = extract_injected_content_from_script(full_html, &hidden);

    let p_sel = Selector::parse("p").unwrap();
    let mut lines: Vec<String> = Vec::new();
    for child in chapter.children() {
        let elem = match ElementRef::wrap(child) {
            Some(e) => e,
            None => continue,
        };
        let tag = elem.value().name();
        let id = elem.value().attr("id");
        // The JS-injected `contentS` paragraphs render at a placeholder div:
        // older pages use `data-content-truyen-backup`, newer ones attach a
        // shadow root to `content-metruyenhot`. Splice the injected content in
        // wherever that placeholder appears.
        if tag == "div"
            && matches!(
                id,
                Some("data-content-truyen-backup" | "content-metruyenhot")
            )
        {
            for line in &injected {
                if !line.is_empty() && !is_noise(line) {
                    lines.push(line.clone());
                }
            }
            continue;
        }
        if is_hidden_noise_element(&elem, &hidden) {
            continue;
        }
        if tag == "p" || tag == "span" {
            if let Some(text) = extract_text_from_element(&elem)
                && !is_noise(&text)
            {
                lines.push(text);
            }
            continue;
        }
        for descendant in elem.select(&p_sel) {
            if is_hidden_noise_element(&descendant, &hidden) {
                continue;
            }
            if let Some(text) = extract_text_from_element(&descendant)
                && !is_noise(&text)
            {
                lines.push(text);
            }
        }
    }

    let mut normalized: Vec<String> = Vec::new();
    for line in lines {
        let cleaned = clean_text(&line);
        if cleaned.is_empty() || is_noise(&cleaned) {
            continue;
        }
        if normalized
            .last()
            .map(|prev| prev == &cleaned)
            .unwrap_or(false)
        {
            continue;
        }
        normalized.push(cleaned);
    }

    Ok(ChapterContent {
        novel_title,
        chapter_title,
        paragraphs: normalized,
    })
}
