//! Recovering chapter prose from an xtruyen chapter page.
//!
//! The page serves an empty reading container and ships the text in an inline
//! script as one encoded string. Everything here is pure, so it is unit tested
//! inline against the same fixtures the adapter uses.

use std::io::Read;

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use flate2::read::ZlibDecoder;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::utils::{clean_text, is_noise};

/// Standard base64, decoding whether or not the payload carries its padding.
/// The site emits padded payloads today, but the padding is the one part of the
/// string its own decoder never looks at, so accepting both costs nothing and
/// removes a way for a run to fail on prose it already holds.
static BASE64: Lazy<GeneralPurpose> = Lazy::new(|| {
    GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    )
});

/// The alphabet the encoded payload is written in, used when the page does not
/// carry its own copy.
const DEFAULT_PAYLOAD_ALPHABET: &str =
    "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_";

/// Standard base64, the alphabet the payload is translated into before it is
/// decoded. Used when the page does not carry its own copy.
const DEFAULT_BASE64_ALPHABET: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Matches the `data_x` assignment holding the encoded chapter payload.
static DATA_X_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"data_x\s*=\s*"([A-Za-z0-9+/=_-]+)""#).unwrap());

/// Matches any single-quoted 64-character string literal, which is the shape
/// both alphabets take inside the page's obfuscated string array. Matching on
/// length rather than on the array index keeps this working when the array is
/// reordered, which it is on every rebuild of the site's script.
static ALPHABET_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"'([A-Za-z0-9+/_-]{64})'").unwrap());

/// Matches the promotional block the site's own script removes before showing
/// the chapter. It is the one piece of markup that travels inside the payload
/// rather than beside it, so the strip has to happen here too.
static PROMO_BLOCK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<div[^>]*class="[^"]*native-stories[^"]*"[^>]*>.*?</div>"#).unwrap()
});

/// Matches one run of `<br>` separators. The site's script flushes a paragraph
/// on every `br` it meets, so a single break is a boundary and a run of them is
/// the same boundary rather than a run of empty paragraphs.
static BREAK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)(?:\s*<br\s*/?>\s*)+").unwrap());

/// Matches one HTML tag. The decoded payload carries only `br` and `div`, so a
/// strip beats reaching for a parser.
static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]*>").unwrap());

/// Extract the encoded payload from a chapter page.
pub(super) fn extract_payload(page_html: &str) -> Result<String> {
    DATA_X_RE
        .captures(page_html)
        .map(|captures| captures[1].to_string())
        .ok_or_else(|| anyhow!("chapter page carries no encoded payload"))
}

/// Extract the payload alphabet and the base64 alphabet from a chapter page,
/// falling back to the compiled-in pair when the page carries neither. The two
/// are told apart by their symbols rather than by their position in the page's
/// string array: only standard base64 contains `+` and `/`.
pub(super) fn extract_alphabets(page_html: &str) -> (String, String) {
    let mut payload_alphabet = None;
    let mut base64_alphabet = None;
    for captures in ALPHABET_RE.captures_iter(page_html) {
        let literal = captures[1].to_string();
        if literal.contains('+') || literal.contains('/') {
            base64_alphabet.get_or_insert(literal);
        } else {
            payload_alphabet.get_or_insert(literal);
        }
    }
    (
        payload_alphabet.unwrap_or_else(|| DEFAULT_PAYLOAD_ALPHABET.to_string()),
        base64_alphabet.unwrap_or_else(|| DEFAULT_BASE64_ALPHABET.to_string()),
    )
}

/// Translate an encoded payload from its own alphabet into standard base64.
/// A character in neither alphabet is passed through untouched, which is what
/// preserves base64 padding.
pub(super) fn translate(payload: &str, from: &str, to: &str) -> String {
    let from: Vec<char> = from.chars().collect();
    let to: Vec<char> = to.chars().collect();
    payload
        .chars()
        .map(|ch| {
            from.iter()
                .position(|candidate| *candidate == ch)
                .and_then(|index| to.get(index).copied())
                .unwrap_or(ch)
        })
        .collect()
}

/// Decode and inflate a translated payload into the chapter's raw markup.
pub(super) fn inflate(translated: &str) -> Result<String> {
    let compressed = BASE64
        .decode(translated)
        .context("translated payload is not valid base64")?;
    let mut markup = String::new();
    ZlibDecoder::new(&compressed[..])
        .read_to_string(&mut markup)
        .context("payload did not inflate as a zlib stream")?;
    Ok(markup)
}

/// Split decoded chapter markup into ordered paragraphs. Stripping tags before
/// [`clean_text`] decodes entities is load-bearing: decoding first would turn an
/// escaped `&lt;b&gt;` in the prose into a tag the strip then swallows, eating
/// the text between the brackets.
fn split_paragraphs(markup: &str) -> Vec<String> {
    let without_promo = PROMO_BLOCK_RE.replace_all(markup, " ");
    BREAK_RE
        .split(&without_promo)
        .map(|piece| clean_text(&TAG_RE.replace_all(piece, " ")))
        .filter(|paragraph| !is_noise(paragraph))
        .collect()
}

/// Recover a chapter's paragraphs from its page, in reading order.
pub(super) fn chapter_paragraphs(page_html: &str) -> Result<Vec<String>> {
    let payload = extract_payload(page_html)?;
    let (payload_alphabet, base64_alphabet) = extract_alphabets(page_html);
    let markup = inflate(&translate(&payload, &payload_alphabet, &base64_alphabet))?;
    Ok(split_paragraphs(&markup))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAPTER_HTML: &str = include_str!("../../../tests/fixtures/xtruyen_chapter.html");
    const SUFFIXED_HTML: &str =
        include_str!("../../../tests/fixtures/xtruyen_chapter_suffixed.html");

    #[test]
    fn extract_payload_reads_the_data_x_assignment() {
        let payload = extract_payload(CHAPTER_HTML).unwrap();
        assert!(payload.starts_with("udG"), "got {payload}");
        assert_eq!(payload.len(), 276);
    }

    #[test]
    fn extract_payload_errors_when_the_page_carries_none() {
        assert!(extract_payload("<html><body>no script here</body></html>").is_err());
    }

    #[test]
    fn extract_alphabets_reads_both_from_the_page() {
        let (payload_alphabet, base64_alphabet) = extract_alphabets(CHAPTER_HTML);
        assert_eq!(payload_alphabet, DEFAULT_PAYLOAD_ALPHABET);
        assert_eq!(base64_alphabet, DEFAULT_BASE64_ALPHABET);
    }

    #[test]
    fn extract_alphabets_falls_back_when_the_page_carries_none() {
        let (payload_alphabet, base64_alphabet) = extract_alphabets("<html></html>");
        assert_eq!(payload_alphabet, DEFAULT_PAYLOAD_ALPHABET);
        assert_eq!(base64_alphabet, DEFAULT_BASE64_ALPHABET);
    }

    #[test]
    fn extract_alphabets_prefers_a_page_supplied_pair_over_the_defaults() {
        let rotated: String = DEFAULT_PAYLOAD_ALPHABET.chars().rev().collect();
        let page =
            format!("<script>const a = ['{rotated}', '{DEFAULT_BASE64_ALPHABET}'];</script>");
        let (payload_alphabet, _) = extract_alphabets(&page);
        assert_eq!(payload_alphabet, rotated);
    }

    #[test]
    fn translate_maps_between_the_two_alphabets() {
        assert_eq!(
            translate("0", DEFAULT_PAYLOAD_ALPHABET, DEFAULT_BASE64_ALPHABET),
            "A"
        );
        assert_eq!(
            translate("A", DEFAULT_BASE64_ALPHABET, DEFAULT_PAYLOAD_ALPHABET),
            "0"
        );
    }

    #[test]
    fn translate_leaves_a_character_outside_the_alphabet_alone() {
        assert_eq!(
            translate("0=", DEFAULT_PAYLOAD_ALPHABET, DEFAULT_BASE64_ALPHABET),
            "A=",
            "base64 padding is not part of either alphabet"
        );
    }

    #[test]
    fn inflate_rejects_input_that_is_not_a_deflate_stream() {
        let translated = translate("AAAA", DEFAULT_PAYLOAD_ALPHABET, DEFAULT_BASE64_ALPHABET);
        assert!(inflate(&translated).is_err());
    }

    #[test]
    fn chapter_paragraphs_recovers_the_prose_in_order() {
        let paragraphs = chapter_paragraphs(CHAPTER_HTML).unwrap();
        assert_eq!(
            paragraphs,
            vec![
                "Đoạn văn thử nghiệm thứ nhất của chương một.",
                "Đoạn văn thử nghiệm thứ hai, có dấu & và một phép so sánh a < b.",
                "Đoạn văn thử nghiệm thứ ba và cuối cùng.",
            ]
        );
    }

    #[test]
    fn chapter_paragraphs_drops_the_promotional_block_the_site_strips_client_side() {
        let paragraphs = chapter_paragraphs(CHAPTER_HTML).unwrap();
        assert!(
            !paragraphs.iter().any(|p| p.contains("quảng cáo")),
            "the native-stories block reached the prose: {paragraphs:?}"
        );
    }

    #[test]
    fn chapter_paragraphs_keeps_the_advertising_script_variable_out_of_the_prose() {
        let paragraphs = chapter_paragraphs(CHAPTER_HTML).unwrap();
        assert!(
            !paragraphs.iter().any(|p| p.contains("example.invalid")),
            "markup from an ads variable reached the prose: {paragraphs:?}"
        );
    }

    #[test]
    fn chapter_paragraphs_reads_a_second_fixture_with_its_own_payload() {
        assert_eq!(
            chapter_paragraphs(SUFFIXED_HTML).unwrap(),
            vec![
                "Đoạn mở đầu của chương mở rộng.",
                "Đoạn thứ hai của chương mở rộng.",
            ]
        );
    }

    #[test]
    fn chapter_paragraphs_errors_when_the_payload_cannot_be_decoded() {
        let broken = CHAPTER_HTML.replace("const data_x = \"udG", "const data_x = \"zzz");
        assert!(
            chapter_paragraphs(&broken).is_err(),
            "a corrupted payload must fail rather than yield an empty chapter"
        );
    }

    #[test]
    fn chapter_paragraphs_errors_when_the_page_carries_no_payload() {
        assert!(chapter_paragraphs("<html><body></body></html>").is_err());
    }
}
