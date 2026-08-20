//! Response types for the four `/wp-json/app/v1/` endpoints this adapter
//! uses. Field names and nullability mirror what the live host returns;
//! unknown fields are ignored, so the site is free to add more.

use serde::{Deserialize, Deserializer};

/// A WordPress taxonomy term. Author, status and type all arrive in this
/// shape and only the display name is of any use here.
#[derive(Debug, Deserialize)]
pub(super) struct Term {
    pub(super) name: String,
}

/// Deserialize a taxonomy field that is an object when the term exists and an
/// empty string when it does not: WordPress serializes an absent term as `""`,
/// not `null`, which a plain `Option<Term>` rejects outright and which took
/// down the whole book payload for the three authorless books on the site.
/// Anything that is not an object counts as absent, so `null`, `false` and an
/// empty array are covered too.
fn optional_term<'de, D>(deserializer: D) -> Result<Option<Term>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_object() {
        serde_json::from_value(value)
            .map(Some)
            .map_err(serde::de::Error::custom)
    } else {
        Ok(None)
    }
}

/// The book endpoint's payload. Returned unenveloped, unlike every listing.
#[derive(Debug, Deserialize)]
pub(super) struct Book {
    pub(super) id: u64,
    pub(super) title: String,
    pub(super) desc: Option<String>,
    pub(super) cover: Option<String>,
    #[serde(default, deserialize_with = "optional_term")]
    pub(super) author: Option<Term>,
    #[serde(default, deserialize_with = "optional_term")]
    pub(super) status: Option<Term>,
}

/// Paging block attached to every listing response. The server reports the
/// page count it actually applied, which is not necessarily what was asked
/// for: the host silently caps `per_page` at 200.
#[derive(Debug, Deserialize)]
pub(super) struct Pagination {
    pub(super) total_pages: u32,
}

/// One entry in the chapter listing. `ID` is the database id the ticket and
/// content endpoints are keyed on; `index` is the chapter number readers see.
/// The sibling `chapter_id` field is always 0 on the live host and ignored.
#[derive(Debug, Deserialize)]
pub(super) struct ChapterListItem {
    #[serde(rename = "ID")]
    pub(super) id: u64,
    pub(super) index: u32,
    pub(super) title: String,
}

/// One page of the chapter listing.
#[derive(Debug, Deserialize)]
pub(super) struct ChapterListPage {
    pub(super) data: Vec<ChapterListItem>,
    pub(super) pagination: Pagination,
}

/// The short-lived credential the content endpoint demands. `uid` is 0 for
/// anonymous callers and is not needed on the follow-up request.
#[derive(Debug, Deserialize)]
pub(super) struct Ticket {
    pub(super) nonce: String,
    pub(super) exp: i64,
    pub(super) sig: String,
}

/// The content endpoint's payload. `content` is plain text with no markup at
/// all, and `can_read` is the entitlement flag: false means the chapter
/// exists but is gated behind a purchase or VIP status.
#[derive(Debug, Deserialize)]
pub(super) struct ChapterContentResponse {
    pub(super) title: String,
    pub(super) content: String,
    pub(super) can_read: bool,
}

/// The standard WordPress REST error envelope, returned on every non-2xx.
/// `code` is the machine-readable discriminator (`rate_limited`,
/// `ticket_invalid`, `not_found`).
#[derive(Debug, Deserialize)]
pub(super) struct ApiError {
    pub(super) code: String,
    pub(super) message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOK_JSON: &str = include_str!("../../../tests/fixtures/khodocsach_book.json");
    const PAGE1_JSON: &str = include_str!("../../../tests/fixtures/khodocsach_chapters_page1.json");
    const TICKET_JSON: &str = include_str!("../../../tests/fixtures/khodocsach_ticket.json");
    const CONTENT_JSON: &str =
        include_str!("../../../tests/fixtures/khodocsach_chapter_content.json");

    #[test]
    fn book_deserializes_with_nested_terms() {
        let book: Book = serde_json::from_str(BOOK_JSON).unwrap();
        assert_eq!(book.id, 83420);
        assert_eq!(book.title, "Một Truyện Thử Nghiệm");
        assert_eq!(book.author.unwrap().name, "Tác Giả Thử Nghiệm");
        assert_eq!(book.status.unwrap().name, "Đang cập nhật");
        assert!(book.desc.unwrap().contains("<p>"));
        assert!(book.cover.unwrap().ends_with("-cover.jpg"));
    }

    #[test]
    fn book_tolerates_a_null_description_and_cover() {
        let book: Book =
            serde_json::from_str(r#"{"id":1,"title":"T","desc":null,"cover":null}"#).unwrap();
        assert!(book.desc.is_none());
        assert!(book.cover.is_none());
        assert!(book.author.is_none());
        assert!(book.status.is_none());
    }

    #[test]
    fn book_treats_an_empty_string_term_as_absent() {
        let book: Book =
            serde_json::from_str(r#"{"id":1,"title":"T","author":"","status":""}"#).unwrap();
        assert!(book.author.is_none());
        assert!(book.status.is_none());
    }

    #[test]
    fn book_treats_a_missing_term_key_as_absent() {
        let book: Book = serde_json::from_str(r#"{"id":1,"title":"T"}"#).unwrap();
        assert!(book.author.is_none());
        assert!(book.status.is_none());
    }

    #[test]
    fn book_treats_other_non_object_terms_as_absent() {
        let book: Book =
            serde_json::from_str(r#"{"id":1,"title":"T","author":false,"status":[]}"#).unwrap();
        assert!(book.author.is_none());
        assert!(book.status.is_none());
    }

    #[test]
    fn book_still_reads_a_populated_term_after_the_empty_string_tolerance() {
        let book: Book =
            serde_json::from_str(r#"{"id":1,"title":"T","author":{"name":"A"}}"#).unwrap();
        assert_eq!(book.author.unwrap().name, "A");
    }

    #[test]
    fn chapter_list_page_reads_the_uppercase_id_and_the_page_count() {
        let page: ChapterListPage = serde_json::from_str(PAGE1_JSON).unwrap();
        assert_eq!(page.pagination.total_pages, 2);
        assert_eq!(page.data.len(), 2);
        assert_eq!(page.data[0].id, 127363);
        assert_eq!(page.data[0].index, 3);
        assert_eq!(page.data[0].title, "Chương 3: Ba");
    }

    #[test]
    fn ticket_deserializes_every_field_the_content_hop_needs() {
        let ticket: Ticket = serde_json::from_str(TICKET_JSON).unwrap();
        assert_eq!(ticket.nonce, "1dcaaedc6f27a0d1");
        assert_eq!(ticket.exp, 1787044275);
        assert_eq!(ticket.sig, "5f26adf064d37e4047b5bf70");
    }

    #[test]
    fn chapter_content_response_carries_the_entitlement_flag() {
        let content: ChapterContentResponse = serde_json::from_str(CONTENT_JSON).unwrap();
        assert_eq!(content.title, "Chương 1: Một");
        assert!(content.can_read);
        assert!(
            content.content.ends_with(r#"<div class="chapter-nav" "#),
            "the wire format wraps chapter prose in ad and nav markup and truncates the \
             last tag mid-way; discarding all that is parser::split_paragraphs' job, so \
             this hop must hand it over intact"
        );
    }

    #[test]
    fn api_error_reads_the_wordpress_envelope() {
        let err: ApiError = serde_json::from_str(
            r#"{"code":"rate_limited","message":"Too many requests","data":{"status":429}}"#,
        )
        .unwrap();
        assert_eq!(err.code, "rate_limited");
        assert_eq!(err.message, "Too many requests");
    }
}
