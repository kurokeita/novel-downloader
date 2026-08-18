mod chapter;
mod parser;
mod types;

pub use chapter::crawl_chapter;
pub use parser::{build_html_document, escape_html};
pub use types::{
    CrawlChapterParams, CrawlResult, CrawlStatus, ExistingChapterDecision, ExistingFilePolicy,
};
