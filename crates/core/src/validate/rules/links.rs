//! The checks of links and bookmarks.

use std::collections::HashSet;

use super::{collect_anchors, Checker, MAX_BOOKMARK_LEVEL, MAX_BOOKMARK_TITLE, MAX_LINK};
use crate::error::ValidationCode as Code;
use crate::schema::Bookmark;

impl Checker<'_> {
    pub(super) fn link(&mut self, link: &str) {
        if link.is_empty() || link.chars().count() > MAX_LINK {
            self.err(
                Code::InvalidValue,
                Some("link"),
                format!("a link must be 1 to {MAX_LINK} characters long"),
            );
            return;
        }
        if let Some(id) = link.strip_prefix('#') {
            let doc = self.doc;
            let anchors = self.anchors.get_or_insert_with(|| {
                let mut out = HashSet::new();
                for n in doc.pages.iter().flat_map(|p| &p.children) {
                    if !n.hidden() {
                        collect_anchors(n, &mut out);
                    }
                }
                out
            });
            if !anchors.contains(id) {
                self.err(
                    Code::UnknownAnchor,
                    Some("link"),
                    format!("no displayed node in the page body (flow and overlays) has id `{id}`"),
                );
            }
            return;
        }
        // RFC 3986 scheme: ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"
        let scheme = link.split_once(':').map(|(s, _)| s).filter(|s| {
            let mut c = s.chars();
            c.next().is_some_and(|c| c.is_ascii_alphabetic())
                && c.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
        match scheme {
            None => self.err(
                Code::InvalidValue,
                Some("link"),
                "a link must be `#id` or an absolute URI with a scheme (e.g. `https:`)",
            ),
            Some(s) if s.eq_ignore_ascii_case("javascript") => self.err(
                Code::InvalidValue,
                Some("link"),
                "`javascript:` links are not allowed",
            ),
            Some(_) => {}
        }
    }

    pub(super) fn bookmark(&mut self, b: &Bookmark) {
        self.at("bookmark", |c| {
            if c.in_fixed {
                c.err(
                    Code::InvalidValue,
                    None,
                    "bookmarks only apply to flowing content, not inside headers or footers",
                );
            }
            if b.title.is_empty() || b.title.chars().count() > MAX_BOOKMARK_TITLE {
                c.err(
                    Code::InvalidValue,
                    Some("title"),
                    format!("a bookmark title must be 1 to {MAX_BOOKMARK_TITLE} characters long"),
                );
            }
            if !(1..=MAX_BOOKMARK_LEVEL).contains(&b.level) {
                c.err(
                    Code::OutOfRange,
                    Some("level"),
                    format!("must be within 1..={MAX_BOOKMARK_LEVEL}"),
                );
            }
        });
    }
}
