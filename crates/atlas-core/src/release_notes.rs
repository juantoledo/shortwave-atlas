//! Release notes: the release's Markdown (a release-please body) reduced to the few shapes the
//! update popover draws: headings, paragraphs and bullet lists of plain, bold, italic, code and
//! link spans. Nothing is passed through as HTML: raw HTML is dropped, and links are kept only
//! for `http(s)` addresses. Help shows this build's own section of `CHANGELOG.md` the same way.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum NoteBlock {
    /// `level` 1-6, as in Markdown.
    Heading { level: u8, text: Vec<NoteSpan> },
    Para { text: Vec<NoteSpan> },
    /// Nested lists are flattened into one.
    List { items: Vec<Vec<NoteSpan>> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum NoteSpan {
    Text { text: String },
    Strong { text: String },
    Em { text: String },
    Code { text: String },
    /// Always an `http://` or `https://` address.
    Link { text: String, url: String },
}

impl NoteSpan {
    fn text_mut(&mut self) -> &mut String {
        match self {
            NoteSpan::Text { text }
            | NoteSpan::Strong { text }
            | NoteSpan::Em { text }
            | NoteSpan::Code { text }
            | NoteSpan::Link { text, .. } => text,
        }
    }
}

#[derive(Default)]
struct Builder {
    blocks: Vec<NoteBlock>,
    spans: Vec<NoteSpan>,
    items: Vec<Vec<NoteSpan>>,
    lists: usize,
    strong: usize,
    em: usize,
    /// Inside a kept link: its address and the text so far.
    link: Option<(String, String)>,
    code_block: Option<String>,
}

impl Builder {
    fn push_text(&mut self, s: &str) {
        if let Some((_, text)) = &mut self.link {
            text.push_str(s);
            return;
        }
        let span = |text: String| {
            if self.strong > 0 {
                NoteSpan::Strong { text }
            } else if self.em > 0 {
                NoteSpan::Em { text }
            } else {
                NoteSpan::Text { text }
            }
        };
        self.push(span(s.to_string()));
    }

    /// Adds a span, joining it to the last one when they look the same.
    fn push(&mut self, span: NoteSpan) {
        match (self.spans.last_mut(), &span) {
            (Some(NoteSpan::Text { text: a }), NoteSpan::Text { text: b })
            | (Some(NoteSpan::Strong { text: a }), NoteSpan::Strong { text: b })
            | (Some(NoteSpan::Em { text: a }), NoteSpan::Em { text: b }) => a.push_str(b),
            _ => self.spans.push(span),
        }
    }

    /// The spans written since the last block, with runs of whitespace collapsed and the ends trimmed.
    fn take_spans(&mut self) -> Vec<NoteSpan> {
        let mut spans: Vec<NoteSpan> = std::mem::take(&mut self.spans)
            .into_iter()
            .map(|mut s| {
                if !matches!(s, NoteSpan::Code { .. }) {
                    let t = s.text_mut();
                    let ends = (t.starts_with(char::is_whitespace), t.ends_with(char::is_whitespace));
                    let mut words = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !words.is_empty() {
                        if ends.0 {
                            words.insert(0, ' ');
                        }
                        if ends.1 {
                            words.push(' ');
                        }
                    } else if ends.0 || ends.1 {
                        words.push(' ');
                    }
                    *t = words;
                }
                s
            })
            .collect();
        if let Some(first) = spans.first_mut() {
            let t = first.text_mut();
            *t = t.trim_start().to_string();
        }
        if let Some(last) = spans.last_mut() {
            let t = last.text_mut();
            *t = t.trim_end().to_string();
        }
        spans.retain_mut(|s| !s.text_mut().is_empty());
        spans
    }

    /// Ends the current list item, or (outside a list) the current paragraph.
    fn flush(&mut self) {
        let spans = self.take_spans();
        if spans.is_empty() {
            return;
        }
        if self.lists > 0 {
            self.items.push(spans);
        } else {
            self.blocks.push(NoteBlock::Para { text: spans });
        }
    }

    fn event(&mut self, ev: Event) {
        if let Some(code) = &mut self.code_block {
            match ev {
                Event::Text(t) => code.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    let text = std::mem::take(code).trim_end().to_string();
                    self.code_block = None;
                    self.flush();
                    if !text.is_empty() {
                        self.push(NoteSpan::Code { text });
                        self.flush();
                    }
                }
                _ => {}
            }
            return;
        }
        match ev {
            Event::Start(Tag::Heading { .. }) => self.flush(),
            Event::End(TagEnd::Heading(level)) => {
                let text = self.take_spans();
                if !text.is_empty() {
                    self.blocks.push(NoteBlock::Heading { level: level as u8, text });
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if self.lists > 0 {
                    // a loose list: the item's paragraphs run on
                    self.push_text(" ");
                } else {
                    self.flush();
                }
            }
            Event::Start(Tag::List(_)) => {
                self.flush();
                self.lists += 1;
            }
            Event::End(TagEnd::List(_)) => {
                self.flush();
                self.lists -= 1;
                if self.lists == 0 && !self.items.is_empty() {
                    self.blocks.push(NoteBlock::List { items: std::mem::take(&mut self.items) });
                }
            }
            Event::Start(Tag::Item) | Event::End(TagEnd::Item) => self.flush(),
            Event::Start(Tag::CodeBlock(_)) => self.code_block = Some(String::new()),
            Event::Start(Tag::Strong) => self.strong += 1,
            Event::End(TagEnd::Strong) => self.strong = self.strong.saturating_sub(1),
            Event::Start(Tag::Emphasis) => self.em += 1,
            Event::End(TagEnd::Emphasis) => self.em = self.em.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) if self.link.is_none() && is_web(&dest_url) => {
                self.link = Some((dest_url.to_string(), String::new()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((url, text)) = self.link.take() {
                    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    let text = if text.is_empty() { url.clone() } else { text };
                    self.push(NoteSpan::Link { text, url });
                }
            }
            Event::Text(t) => self.push_text(&t),
            Event::Code(t) => {
                if self.link.is_some() {
                    self.push_text(&t);
                } else {
                    self.push(NoteSpan::Code { text: t.to_string() });
                }
            }
            Event::SoftBreak | Event::HardBreak => self.push_text(" "),
            // raw HTML (comments, tags) is dropped; rules, images' addresses and the rest are ignored
            _ => {}
        }
    }
}

fn is_web(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// This build's `CHANGELOG.md` (release-please writes it, newest version first).
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// This build's own notes, from its section of the changelog.
pub fn this_version() -> Vec<NoteBlock> {
    section(CHANGELOG, env!("CARGO_PKG_VERSION"))
}

/// One version's section of a changelog: its `## [1.2.3](…) (date)` heading and what follows,
/// up to the next version's. Empty if the changelog has no such version.
fn section(md: &str, version: &str) -> Vec<NoteBlock> {
    let is_version = |b: &NoteBlock| matches!(b, NoteBlock::Heading { level: ..=2, .. });
    let names = |b: &NoteBlock| match b {
        NoteBlock::Heading { level: ..=2, text } => {
            let mut words = String::new();
            for s in text {
                let (NoteSpan::Text { text: t } | NoteSpan::Strong { text: t } | NoteSpan::Em { text: t } | NoteSpan::Code { text: t } | NoteSpan::Link { text: t, .. }) = s;
                words.push_str(t);
            }
            words.split(|c: char| c.is_whitespace() || "[]()".contains(c)).any(|w| w.trim_start_matches('v') == version)
        }
        _ => false,
    };
    let blocks = parse(md);
    let Some(start) = blocks.iter().position(names) else { return vec![] };
    let end = blocks[start + 1..].iter().position(is_version).map_or(blocks.len(), |n| start + 1 + n);
    blocks[start..end].to_vec()
}

/// The release notes' Markdown, as blocks. Empty for empty notes.
pub fn parse(md: &str) -> Vec<NoteBlock> {
    let mut b = Builder::default();
    for ev in Parser::new(md) {
        b.event(ev);
    }
    b.flush();
    b.blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> NoteSpan {
        NoteSpan::Text { text: t.into() }
    }
    fn strong(t: &str) -> NoteSpan {
        NoteSpan::Strong { text: t.into() }
    }
    fn link(t: &str, url: &str) -> NoteSpan {
        NoteSpan::Link { text: t.into(), url: url.into() }
    }

    /// What release-please writes (the 0.13.0 section of CHANGELOG.md).
    const RELEASE_PLEASE: &str = "## [0.13.0](https://github.com/juantoledo/shortwave-atlas/compare/v0.12.2...v0.13.0) (2026-10-19)


### Features

* **net:** adaptive stream buffer, drawn as a funnel that picks its size ([c11cf3c](https://github.com/juantoledo/shortwave-atlas/commit/c11cf3c9797ef82fc8769f818605330041edf74c))


### Bug Fixes

* **deps:** drop the updater's zip feature (RUSTSEC-2026-0369) ([a2193ad](https://github.com/juantoledo/shortwave-atlas/commit/a2193ad1b555f68a81475d0df24d44f28e3e07f4))
* **ui:** stop the flat map crashing the GPU process ([8d1a019](https://github.com/juantoledo/shortwave-atlas/commit/8d1a0199a9d6bb762fff728a2eb6a5a1ccbcd1ed))
";

    #[test]
    fn release_please_body() {
        let commit = |h: &str| format!("https://github.com/juantoledo/shortwave-atlas/commit/{h}");
        let blocks = parse(RELEASE_PLEASE);
        assert_eq!(
            blocks,
            vec![
                NoteBlock::Heading {
                    level: 2,
                    text: vec![
                        link("0.13.0", "https://github.com/juantoledo/shortwave-atlas/compare/v0.12.2...v0.13.0"),
                        text(" (2026-10-19)"),
                    ],
                },
                NoteBlock::Heading { level: 3, text: vec![text("Features")] },
                NoteBlock::List {
                    items: vec![vec![
                        strong("net:"),
                        text(" adaptive stream buffer, drawn as a funnel that picks its size ("),
                        link("c11cf3c", &commit("c11cf3c9797ef82fc8769f818605330041edf74c")),
                        text(")"),
                    ]],
                },
                NoteBlock::Heading { level: 3, text: vec![text("Bug Fixes")] },
                NoteBlock::List {
                    items: vec![
                        vec![
                            strong("deps:"),
                            text(" drop the updater's zip feature (RUSTSEC-2026-0369) ("),
                            link("a2193ad", &commit("a2193ad1b555f68a81475d0df24d44f28e3e07f4")),
                            text(")"),
                        ],
                        vec![
                            strong("ui:"),
                            text(" stop the flat map crashing the GPU process ("),
                            link("8d1a019", &commit("8d1a0199a9d6bb762fff728a2eb6a5a1ccbcd1ed")),
                            text(")"),
                        ],
                    ],
                },
            ]
        );
    }

    #[test]
    fn paragraphs_and_lists() {
        assert_eq!(
            parse("Features:\n\n* update available banner"),
            vec![
                NoteBlock::Para { text: vec![text("Features:")] },
                NoteBlock::List { items: vec![vec![text("update available banner")]] },
            ]
        );
        // soft breaks join, emphasis and code keep their kind
        assert_eq!(
            parse("one\ntwo *three* `four`"),
            vec![NoteBlock::Para {
                text: vec![text("one two "), NoteSpan::Em { text: "three".into() }, text(" "), NoteSpan::Code { text: "four".into() }],
            }]
        );
        // nested and loose lists flatten into one
        assert_eq!(
            parse("- a\n  - b\n- c\n\n  more\n"),
            vec![NoteBlock::List { items: vec![vec![text("a")], vec![text("b")], vec![text("c more")]] }]
        );
        assert_eq!(parse(""), vec![]);
        assert_eq!(parse("  \n\n"), vec![]);
    }

    #[test]
    fn one_version_of_a_changelog() {
        let older = "## [0.12.2](https://github.com/o/r/compare/v0.12.1...v0.12.2) (2026-10-16)\n\n### Bug Fixes\n\n* older fix\n";
        let newer = "## [0.13.10](https://github.com/o/r/compare/v0.13.0...v0.13.10) (2026-10-20)\n\n* not 0.13.0\n";
        let md = format!("# Changelog\n\n{newer}\n{RELEASE_PLEASE}\n{older}");
        assert_eq!(section(&md, "0.13.0"), parse(RELEASE_PLEASE));
        assert_eq!(section(&md, "0.12.2"), parse(older));
        assert_eq!(section(&md, "0.13.1"), vec![]);
        // the bundled changelog has this build's section
        let mine = this_version();
        assert!(matches!(mine.first(), Some(NoteBlock::Heading { level: 2, .. })), "{:?}", mine.first());
        assert!(mine.len() > 1, "{mine:?}");
    }

    #[test]
    fn nothing_unsafe_survives() {
        assert_eq!(parse("[click](javascript:alert(1))"), vec![NoteBlock::Para { text: vec![text("click")] }]);
        assert_eq!(parse("[x](/relative)"), vec![NoteBlock::Para { text: vec![text("x")] }]);
        assert_eq!(parse("a <b onclick=x>b</b> c"), vec![NoteBlock::Para { text: vec![text("a b c")] }]);
        assert_eq!(parse("<!-- hidden -->\n\nshown"), vec![NoteBlock::Para { text: vec![text("shown")] }]);
        assert_eq!(parse("<https://example.com>"), vec![NoteBlock::Para { text: vec![link("https://example.com", "https://example.com")] }]);
        assert_eq!(
            parse("```\nlet x = 1;\n```"),
            vec![NoteBlock::Para { text: vec![NoteSpan::Code { text: "let x = 1;".into() }] }]
        );
    }
}
