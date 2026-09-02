//! Renders the Markdown that comments are written in.
//!
//! The comment delimiters stay as they are, and so does every other character of the buffer: the
//! text keeps its position, the inline markers are folded away to nothing, and the text they mark
//! gets the matching style. Markup nests, so ``**_`term`_**`` is bold, italic and
//! code at once. The comment that holds the cursor keeps its markers, so it reads as its own
//! source while it is edited.

use std::{any::TypeId, ops::Range, sync::Arc, time::Duration};

use gpui::{App, Context, FontStyle, FontWeight, HighlightStyle, IntoElement as _, UnderlineStyle};
use language::BufferSnapshot;
use multi_buffer::Anchor;
use text::Point;
use theme::ActiveTheme as _;

use crate::{
    Editor, EditorSettings,
    display_map::{Crease, FoldPlaceholder, HighlightKey},
};
use settings::Settings as _;

/// Identifies the folds that take the Markdown markers off the screen, so they can be removed
/// without disturbing the folds of the user.
enum MarkdownCommentFold {}

/// Long enough for the clicks and the drags of one gesture to settle, short enough to read as
/// immediate.
const MARKDOWN_COMMENTS_DEBOUNCE: Duration = Duration::from_millis(50);

/// The markup that applies to a range of a comment. Markup nests, so a range can carry several
/// kinds at once.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MarkdownCommentStyle(u8);

impl MarkdownCommentStyle {
    const BOLD: Self = Self(1 << 0);
    const ITALIC: Self = Self(1 << 1);
    const CODE: Self = Self(1 << 2);
    const LINK: Self = Self(1 << 3);
    const HEADING: Self = Self(1 << 4);

    fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn highlight_style(self, cx: &App) -> HighlightStyle {
        let colors = cx.theme().colors();
        let mut style = HighlightStyle::default();
        if self.contains(Self::BOLD) || self.contains(Self::HEADING) {
            style.font_weight = Some(FontWeight::BOLD);
        }
        if self.contains(Self::ITALIC) {
            style.font_style = Some(FontStyle::Italic);
        }
        if self.contains(Self::HEADING) {
            style.color = Some(colors.text);
        }
        if self.contains(Self::CODE) {
            style.background_color = Some(colors.element_background);
            style.color = Some(colors.text);
        }
        if self.contains(Self::LINK) {
            style.color = Some(colors.link_text_hover);
            style.underline = Some(UnderlineStyle {
                thickness: gpui::px(1.),
                ..Default::default()
            });
        }
        style
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct MarkdownSpans {
    /// The ranges to style, per combination of markup.
    styled: Vec<(MarkdownCommentStyle, Vec<Range<usize>>)>,
    /// The markers to conceal.
    concealed: Vec<Range<usize>>,
}

impl MarkdownSpans {
    fn push_style(&mut self, style: MarkdownCommentStyle, range: Range<usize>) {
        if range.is_empty() || style.is_empty() {
            return;
        }
        match self
            .styled
            .iter_mut()
            .find(|(existing, _)| *existing == style)
        {
            Some((_, ranges)) => ranges.push(range),
            None => self.styled.push((style, vec![range])),
        }
    }

    fn conceal(&mut self, range: Range<usize>) {
        if !range.is_empty() {
            self.concealed.push(range);
        }
    }
}

impl Editor {
    pub fn toggle_markdown_comments(
        &mut self,
        _: &crate::actions::ToggleMarkdownComments,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let enabled = self.markdown_comments_enabled(cx);
        self.markdown_comments_enabled = Some(!enabled);
        self.refresh_markdown_comments(cx);
    }

    pub fn markdown_comments_enabled(&self, cx: &App) -> bool {
        self.markdown_comments_enabled
            .unwrap_or_else(|| EditorSettings::get_global(cx).render_markdown_in_comments)
    }

    /// Schedules the refresh that follows a move of the cursor.
    ///
    /// A click lands while the mouse still drives the selection, and folding the markers away
    /// changes the length of the row, which moves the text under the pointer. Waiting for the
    /// selection to settle keeps the comment from switching back and forth under the pointer.
    pub(crate) fn refresh_markdown_comments_debounced(&mut self, cx: &mut Context<Self>) {
        if !self.mode().is_full() || !self.markdown_comments_enabled(cx) {
            return;
        }
        self.refresh_markdown_comments_task = cx.spawn(async move |editor, cx| {
            cx.background_executor()
                .timer(MARKDOWN_COMMENTS_DEBOUNCE)
                .await;
            editor
                .update(cx, |editor, cx| editor.refresh_markdown_comments(cx))
                .ok();
        });
    }

    pub(crate) fn refresh_markdown_comments(&mut self, cx: &mut Context<Self>) {
        if !self.mode().is_full() {
            return;
        }
        // While the mouse is still choosing the selection, the rows it points at must not move.
        if self.has_pending_selection() {
            return;
        }

        let spans = self
            .markdown_comments_enabled(cx)
            .then(|| self.markdown_comment_spans(cx))
            .unwrap_or_default();
        // Moving the cursor inside one comment leaves the same thing to draw. Rebuilding the folds
        // for an unchanged result makes the comment flicker, so nothing is touched here.
        if spans == self.markdown_comment_spans {
            return;
        }
        self.markdown_comment_spans = spans.clone();

        let snapshot = self.buffer().read(cx).snapshot(cx);
        let anchor_range = |range: &Range<usize>| {
            snapshot.anchor_after(multi_buffer::MultiBufferOffset(range.start))
                ..snapshot.anchor_before(multi_buffer::MultiBufferOffset(range.end))
        };

        let styles = spans
            .styled
            .iter()
            .map(|(style, _)| *style)
            .collect::<Vec<_>>();
        for (style, ranges) in &spans.styled {
            self.highlight_text(
                HighlightKey::MarkdownComment(*style),
                ranges.iter().map(anchor_range).collect(),
                style.highlight_style(cx),
                cx,
            );
        }
        for style in std::mem::replace(&mut self.markdown_comment_styles, styles) {
            if !self.markdown_comment_styles.contains(&style) {
                self.highlight_text(
                    HighlightKey::MarkdownComment(style),
                    Vec::new(),
                    HighlightStyle::default(),
                    cx,
                );
            }
        }

        let concealed = spans
            .concealed
            .iter()
            .map(anchor_range)
            .collect::<Vec<Range<Anchor>>>();
        let type_id = TypeId::of::<MarkdownCommentFold>();
        let previous = std::mem::replace(&mut self.markdown_comment_folds, concealed.clone());
        let placeholder = FoldPlaceholder {
            // An empty placeholder takes the marker off the screen completely: it draws nothing
            // and stands for no character, so the text around it closes up.
            render: Arc::new(|_, _, _| gpui::Empty.into_any_element()),
            constrain_width: false,
            merge_adjacent: false,
            type_tag: Some(type_id),
            collapsed_text: Some("".into()),
        };
        let creases = concealed
            .into_iter()
            .map(|range| Crease::simple(range, placeholder.clone()))
            .collect::<Vec<_>>();
        self.display_map.update(cx, |display_map, cx| {
            display_map.remove_folds_with_type(previous, type_id, cx);
            display_map.fold(creases, cx);
        });
        cx.notify();
    }

    fn markdown_comment_spans(&self, cx: &App) -> MarkdownSpans {
        let multi_buffer = self.buffer().read(cx);
        let Some(buffer) = multi_buffer.as_singleton() else {
            return MarkdownSpans::default();
        };
        let snapshot = buffer.read(cx).snapshot();
        let multi_buffer_snapshot = multi_buffer.snapshot(cx);
        let cursor_rows = self
            .selections
            .disjoint_anchors()
            .iter()
            .filter_map(|selection| {
                let (_, buffer_offset) =
                    multi_buffer_snapshot.point_to_buffer_offset(selection.head())?;
                Some(snapshot.offset_to_point(buffer_offset.0).row)
            })
            .collect::<Vec<_>>();

        let mut spans = MarkdownSpans::default();
        let mut row = 0;
        let last_row = snapshot.max_point().row;
        while row <= last_row {
            let Some(block) = comment_block_at(&snapshot, row, last_row) else {
                row += 1;
                continue;
            };
            let holds_cursor = cursor_rows
                .iter()
                .any(|cursor_row| block.contains(cursor_row));
            if !holds_cursor {
                for comment_row in block.clone() {
                    parse_comment_row(&snapshot, comment_row, &mut spans);
                }
            }
            row = block.end;
        }
        spans
    }
}

/// The rows of the comment block that starts at `row`, if that row is a comment.
fn comment_block_at(
    snapshot: &BufferSnapshot,
    row: u32,
    last_row: u32,
) -> Option<std::ops::Range<u32>> {
    if !is_comment_row(snapshot, row) {
        return None;
    }
    let mut end = row + 1;
    while end <= last_row && is_comment_row(snapshot, end) {
        end += 1;
    }
    Some(row..end)
}

fn is_comment_row(snapshot: &BufferSnapshot, row: u32) -> bool {
    let indent = snapshot.indent_size_for_line(row).len;
    let line_len = snapshot.line_len(row);
    if indent >= line_len {
        return false;
    }
    snapshot
        .language_scope_at(Point::new(row, indent))
        .is_some_and(|scope| scope.override_name() == Some("comment"))
}

/// Parses the Markdown of one comment row, in buffer offsets.
fn parse_comment_row(snapshot: &BufferSnapshot, row: u32, spans: &mut MarkdownSpans) {
    let line_start = snapshot.point_to_offset(Point::new(row, 0));
    let line: String = snapshot
        .text_for_range(line_start..line_start + snapshot.line_len(row) as usize)
        .collect();
    let Some(text_start) = comment_text_start(&line) else {
        return;
    };
    parse_inline_markdown(
        &line[text_start..],
        line_start + text_start,
        MarkdownCommentStyle::default(),
        spans,
    );
}

/// The byte offset where the text of a comment row starts, after its delimiter.
///
/// Returns `None` when the row carries no text of its own, such as the `/**` row of a doc comment.
fn comment_text_start(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    let delimiter = ["///", "//!", "//", "/**", "/*", "*/", "*", "#"]
        .into_iter()
        .find(|delimiter| rest.starts_with(delimiter))?;
    let after_delimiter = indent + delimiter.len();
    let text = &line[after_delimiter..];
    if text.trim().is_empty() {
        return None;
    }
    // Only a delimiter followed by a space opens text; `*/` and `**bold**` are not delimiters of
    // a text that starts right at them.
    let spaces = text.len() - text.trim_start_matches(' ').len();
    (spaces > 0).then_some(after_delimiter + spaces)
}

/// Applies the inline Markdown of `text`, whose first byte sits at `offset` in the buffer.
///
/// `outer` is the markup that the markers around this text already gave it, so nested markup adds
/// to it rather than replacing it.
fn parse_inline_markdown(
    text: &str,
    offset: usize,
    outer: MarkdownCommentStyle,
    spans: &mut MarkdownSpans,
) {
    if let Some(level) = heading_level(text) {
        spans.conceal(offset..offset + level + 1);
        parse_inline_markdown(
            &text[level + 1..],
            offset + level + 1,
            outer.with(MarkdownCommentStyle::HEADING),
            spans,
        );
        return;
    }

    let bytes = text.as_bytes();
    let mut index = 0;
    // The text between nested markers still carries the markup of the markers around it.
    let mut plain_start = 0;

    while index < bytes.len() {
        let nested = match bytes[index] {
            b'`' => find_closing(bytes, index + 1, b'`').map(|end| Nested {
                content: index + 1..end,
                end: end + 1,
                style: MarkdownCommentStyle::CODE,
                // A code span holds no markup of its own.
                parse_content: false,
            }),
            marker @ (b'*' | b'_') => {
                let run = marker_run(bytes, index, marker).min(3);
                opens_emphasis(bytes, index, run, marker)
                    .then(|| find_marker_run(bytes, index + run, marker, run))
                    .flatten()
                    .map(|end| Nested {
                        content: index + run..end,
                        end: end + run,
                        style: match run {
                            1 => MarkdownCommentStyle::ITALIC,
                            2 => MarkdownCommentStyle::BOLD,
                            _ => MarkdownCommentStyle::BOLD.with(MarkdownCommentStyle::ITALIC),
                        },
                        parse_content: true,
                    })
            }
            b'[' => find_closing(bytes, index + 1, b']')
                .filter(|text_end| bytes.get(text_end + 1) == Some(&b'('))
                .and_then(|text_end| {
                    let url_end = find_closing(bytes, text_end + 2, b')')?;
                    Some(Nested {
                        content: index + 1..text_end,
                        end: url_end + 1,
                        style: MarkdownCommentStyle::LINK,
                        parse_content: true,
                    })
                }),
            _ => None,
        };

        let Some(nested) = nested else {
            index += 1;
            continue;
        };

        spans.push_style(outer, offset + plain_start..offset + index);
        spans.conceal(offset + index..offset + nested.content.start);
        let style = outer.with(nested.style);
        if nested.parse_content {
            parse_inline_markdown(
                &text[nested.content.clone()],
                offset + nested.content.start,
                style,
                spans,
            );
        } else {
            spans.push_style(
                style,
                offset + nested.content.start..offset + nested.content.end,
            );
        }
        spans.conceal(offset + nested.content.end..offset + nested.end);
        index = nested.end;
        plain_start = index;
    }
    spans.push_style(outer, offset + plain_start..offset + bytes.len());
}

/// Markup found inside a text: the range it applies to, and where it ends.
struct Nested {
    content: Range<usize>,
    end: usize,
    style: MarkdownCommentStyle,
    parse_content: bool,
}

fn heading_level(text: &str) -> Option<usize> {
    let level = text.len() - text.trim_start_matches('#').len();
    (1..=6).contains(&level).then_some(level)?;
    (text.as_bytes().get(level) == Some(&b' ')).then_some(level)
}

/// Whether the run of markers at `index` can open emphasis.
///
/// A marker followed by a space marks nothing (`a * b`), and `_` marks nothing inside a word, so
/// that `snake_case_names` in comments stay as they are.
fn opens_emphasis(bytes: &[u8], index: usize, run: usize, marker: u8) -> bool {
    if run == 0 {
        return false;
    }
    let after = bytes.get(index + run);
    if after.is_none_or(|byte| byte.is_ascii_whitespace()) {
        return false;
    }
    if marker == b'_' {
        let before = index.checked_sub(1).map(|previous| bytes[previous]);
        if before.is_some_and(|byte| byte.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

/// Whether the run of markers at `index` can close emphasis: `a *b * c` closes nothing.
fn closes_emphasis(bytes: &[u8], index: usize, marker: u8) -> bool {
    let before = index.checked_sub(1).map(|previous| bytes[previous]);
    if before.is_none_or(|byte| byte.is_ascii_whitespace()) {
        return false;
    }
    if marker == b'_' {
        let after = bytes.get(index + marker_run(bytes, index, marker));
        if after.is_some_and(|byte| byte.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

fn find_closing(bytes: &[u8], from: usize, closing: u8) -> Option<usize> {
    (from..bytes.len()).find(|index| bytes[*index] == closing)
}

fn marker_run(bytes: &[u8], from: usize, marker: u8) -> usize {
    bytes[from..]
        .iter()
        .take_while(|byte| **byte == marker)
        .count()
}

/// Finds the run of `count` markers that closes the emphasis opened at `from`.
fn find_marker_run(bytes: &[u8], from: usize, marker: u8, count: usize) -> Option<usize> {
    let mut index = from;
    while index < bytes.len() {
        if bytes[index] == marker {
            let run = marker_run(bytes, index, marker);
            if run == count && index > from && closes_emphasis(bytes, index, marker) {
                return Some(index);
            }
            index += run;
            continue;
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The styled ranges of a comment line as `(text, markup)`, in the order they appear.
    fn spans_of(text: &str) -> (Vec<(String, Vec<&'static str>)>, Vec<String>) {
        let mut spans = MarkdownSpans::default();
        let start = comment_text_start(text).unwrap_or(0);
        parse_inline_markdown(
            &text[start..],
            start,
            MarkdownCommentStyle::default(),
            &mut spans,
        );
        let mut styled = spans
            .styled
            .iter()
            .flat_map(|(style, ranges)| {
                ranges.iter().map(move |range| {
                    let names = [
                        (MarkdownCommentStyle::BOLD, "bold"),
                        (MarkdownCommentStyle::ITALIC, "italic"),
                        (MarkdownCommentStyle::CODE, "code"),
                        (MarkdownCommentStyle::LINK, "link"),
                        (MarkdownCommentStyle::HEADING, "heading"),
                    ]
                    .into_iter()
                    .filter(|(flag, _)| style.contains(*flag))
                    .map(|(_, name)| name)
                    .collect::<Vec<_>>();
                    (range.start, text[range.clone()].to_string(), names)
                })
            })
            .collect::<Vec<_>>();
        styled.sort_by_key(|(start, _, _)| *start);
        let concealed = spans
            .concealed
            .iter()
            .map(|range| text[range.clone()].to_string())
            .collect();
        (
            styled
                .into_iter()
                .map(|(_, text, names)| (text, names))
                .collect(),
            concealed,
        )
    }

    #[test]
    fn test_comment_text_start() {
        assert_eq!(comment_text_start("// text"), Some(3));
        assert_eq!(comment_text_start("  /// text"), Some(6));
        assert_eq!(comment_text_start(" * text"), Some(3));
        assert_eq!(comment_text_start("/** text"), Some(4));
        assert_eq!(
            comment_text_start("/**"),
            None,
            "a row with no text of its own"
        );
        assert_eq!(comment_text_start(" */"), None);
        assert_eq!(comment_text_start("plain text"), None);
    }

    #[test]
    fn test_inline_markdown() {
        let (styled, concealed) = spans_of("// a **bold** word");
        assert_eq!(styled, [("bold".to_string(), vec!["bold"])]);
        assert_eq!(concealed, ["**", "**"]);

        let (styled, concealed) = spans_of("// an *italic* and `code` and ***both***");
        assert_eq!(
            styled,
            [
                ("italic".to_string(), vec!["italic"]),
                ("code".to_string(), vec!["code"]),
                ("both".to_string(), vec!["bold", "italic"]),
            ]
        );
        assert_eq!(concealed, ["*", "*", "`", "`", "***", "***"]);

        let (styled, concealed) = spans_of("// see [the docs](https://zed.dev)");
        assert_eq!(styled, [("the docs".to_string(), vec!["link"])]);
        assert_eq!(concealed, ["[", "](https://zed.dev)"]);

        let (styled, _) = spans_of("// ## A heading");
        assert_eq!(styled, [("A heading".to_string(), vec!["heading"])]);
    }

    #[test]
    fn test_nested_markup_combines() {
        let (styled, concealed) = spans_of("// A **_`block instance`_** term");
        assert_eq!(
            styled,
            [("block instance".to_string(), vec!["bold", "italic", "code"])],
            "a code span keeps the emphasis of the markers around it"
        );
        assert_eq!(concealed, ["**", "_", "`", "`", "_", "**"]);

        let (styled, _) = spans_of("// **bold with _italic_ inside**");
        assert_eq!(
            styled,
            [
                ("bold with ".to_string(), vec!["bold"]),
                ("italic".to_string(), vec!["bold", "italic"]),
                (" inside".to_string(), vec!["bold"]),
            ],
            "the text around nested markers keeps the outer markup"
        );

        let (styled, _) = spans_of("// [a **bold** link](https://zed.dev)");
        assert_eq!(
            styled,
            [
                ("a ".to_string(), vec!["link"]),
                ("bold".to_string(), vec!["bold", "link"]),
                (" link".to_string(), vec!["link"]),
            ]
        );

        let (styled, _) = spans_of("// # A **bold** heading");
        assert_eq!(
            styled,
            [
                ("A ".to_string(), vec!["heading"]),
                ("bold".to_string(), vec!["bold", "heading"]),
                (" heading".to_string(), vec!["heading"]),
            ]
        );
    }

    #[test]
    fn test_emphasis_boundaries() {
        let (styled, concealed) = spans_of("// a snake_case_name stays whole");
        assert!(styled.is_empty(), "underscores inside a word mark nothing");
        assert!(concealed.is_empty());

        let (styled, _) = spans_of("// _leading_ and trailing_ and _both_");
        assert_eq!(
            styled,
            [
                ("leading".to_string(), vec!["italic"]),
                ("both".to_string(), vec!["italic"]),
            ]
        );

        let (styled, _) = spans_of("// 2 * 3 * 4 is arithmetic");
        assert!(
            styled.is_empty(),
            "a marker followed by a space marks nothing"
        );

        let (styled, _) = spans_of("// `a * b` keeps its stars");
        assert_eq!(styled, [("a * b".to_string(), vec!["code"])]);
    }

    #[test]
    fn test_unclosed_and_empty_markup() {
        for line in [
            "// **unclosed bold",
            "// `unclosed code",
            "// [unclosed link](",
            "// [text] (spaced)",
            "// a lone ` tick",
            "//",
            "// ####### too many hashes",
            "// #no space after hash",
        ] {
            let (styled, concealed) = spans_of(line);
            assert!(styled.is_empty(), "{line:?} should style nothing");
            assert!(concealed.is_empty(), "{line:?} should conceal nothing");
        }
    }

    #[test]
    fn test_multibyte_text_keeps_offsets() {
        // The offsets are byte offsets into the buffer, so a multi-byte character before a marker
        // must not shift the span.
        let (styled, _) = spans_of("// длинный **жирный** текст");
        assert_eq!(styled, [("жирный".to_string(), vec!["bold"])]);
    }

    #[test]
    fn test_heading_and_link_together() {
        let (styled, concealed) = spans_of("// # See [the docs](https://zed.dev) now");
        assert_eq!(
            styled,
            [
                ("See ".to_string(), vec!["heading"]),
                ("the docs".to_string(), vec!["link", "heading"]),
                (" now".to_string(), vec!["heading"]),
            ]
        );
        assert_eq!(concealed, ["# ", "[", "](https://zed.dev)"]);
    }
}

#[cfg(test)]
mod editor_tests {
    use super::MarkdownCommentFold;
    use crate::{actions::ToggleMarkdownComments, test::editor_test_context::EditorTestContext};
    use gpui::{TestAppContext, UpdateGlobal as _};
    use indoc::indoc;
    use language::rust_lang;
    use settings::SettingsStore;
    use std::any::TypeId;

    async fn markdown_comment_editor(cx: &mut TestAppContext) -> EditorTestContext {
        crate::editor_tests::init_test(cx, |_| {});
        let mut cx = EditorTestContext::new(cx).await;
        cx.update(|_, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.editor.render_markdown_in_comments = Some(true);
                });
            });
        });
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(rust_lang()), cx));
        cx
    }

    #[gpui::test]
    async fn test_markers_are_concealed_outside_the_comment_with_the_cursor(
        cx: &mut TestAppContext,
    ) {
        let mut cx = markdown_comment_editor(cx).await;
        cx.set_state(indoc! {"
            // A **bold** word and `code`.
            fn mainˇ() {}
        "});
        cx.run_until_parked();

        // A folded marker stands as one space in the display text; the element that draws it is
        // empty, so the marker takes no width on the screen.
        assert_eq!(
            cx.display_text(),
            indoc! {"
                // A bold word and code.
                fn main() {}
            "},
            "the markers are concealed while the cursor is elsewhere"
        );
        assert_eq!(
            cx.buffer_text(),
            indoc! {"
                // A **bold** word and `code`.
                fn main() {}
            "},
            "the buffer keeps every character"
        );
    }

    #[gpui::test]
    async fn test_the_comment_with_the_cursor_shows_its_source(cx: &mut TestAppContext) {
        let mut cx = markdown_comment_editor(cx).await;
        cx.set_state(indoc! {"
            // A **bold** word.
            // Another **bold** word.
            fn mainˇ() {}
        "});
        cx.run_until_parked();
        assert_eq!(
            cx.display_text(),
            indoc! {"
                // A bold word.
                // Another bold word.
                fn main() {}
            "}
        );

        // Both rows are one block of comments, so a cursor in either shows the source of both.
        cx.set_state(indoc! {"
            // A **boldˇ** word.
            // Another **bold** word.
            fn main() {}
        "});
        cx.run_until_parked();
        assert_eq!(
            cx.display_text(),
            indoc! {"
                // A **bold** word.
                // Another **bold** word.
                fn main() {}
            "},
            "the comment block that holds the cursor stays as its source"
        );

        // A separate comment block keeps its Markdown rendered.
        cx.set_state(indoc! {"
            // A **bold** word.

            // Another **boldˇ** word.
            fn main() {}
        "});
        cx.run_until_parked();
        assert_eq!(
            cx.display_text(),
            indoc! {"
                // A bold word.

                // Another **bold** word.
                fn main() {}
            "}
        );
    }

    #[gpui::test]
    async fn test_moving_the_cursor_inside_one_comment_changes_nothing(cx: &mut TestAppContext) {
        let mut cx = markdown_comment_editor(cx).await;
        cx.set_state(indoc! {"
            // A **boldˇ** word and `code`.
            fn main() {}
        "});
        cx.run_until_parked();
        let source = cx.display_text();
        let folds_before = cx.update_editor(|editor, _, _| editor.markdown_comment_folds.clone());

        for state in [
            indoc! {"
                // ˇA **bold** word and `code`.
                fn main() {}
            "},
            indoc! {"
                // A **bold** word and `code`.ˇ
                fn main() {}
            "},
        ] {
            cx.set_state(state);
            cx.run_until_parked();
            assert_eq!(
                cx.display_text(),
                source,
                "the comment stays as its source while the cursor moves inside it"
            );
            assert_eq!(
                cx.update_editor(|editor, _, _| editor.markdown_comment_folds.clone()),
                folds_before,
                "the folds are not rebuilt, so the comment does not flicker"
            );
        }
    }

    #[gpui::test]
    async fn test_markers_are_folded_away_to_nothing(cx: &mut TestAppContext) {
        let mut cx = markdown_comment_editor(cx).await;
        cx.set_state(indoc! {"
            // A **bold** word.
            fn mainˇ() {}
        "});
        cx.run_until_parked();

        let folds = cx.update_editor(|editor, _, cx| {
            let snapshot = editor.display_map.update(cx, |map, cx| map.snapshot(cx));
            snapshot
                .folds_in_range(
                    multi_buffer::MultiBufferOffset(0)..snapshot.buffer_snapshot().len(),
                )
                .map(|fold| {
                    (
                        fold.placeholder.type_tag,
                        fold.placeholder.collapsed_text.clone(),
                    )
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(folds.len(), 2, "one fold per marker");
        for (type_tag, collapsed_text) in folds {
            assert_eq!(
                type_tag,
                Some(TypeId::of::<MarkdownCommentFold>()),
                "the markers carry our own placeholder, not the ellipsis of the editor"
            );
            assert_eq!(
                collapsed_text.as_deref(),
                Some(""),
                "an empty placeholder leaves no character and no width behind"
            );
        }
    }

    #[gpui::test]
    async fn test_marker_folds_are_not_persisted(cx: &mut TestAppContext) {
        let mut cx = markdown_comment_editor(cx).await;
        cx.set_state(indoc! {"
            // A **bold** word.
            fn mainˇ() {}
        "});
        cx.run_until_parked();

        // A fold of a feature must never reach the fold restoration of the user: it would come
        // back as a normal fold, with the ellipsis of the editor, in a file where the feature is
        // not even active.
        let persisted = cx.update_editor(|editor, _, cx| {
            let snapshot = editor.display_map.update(cx, |map, cx| map.snapshot(cx));
            snapshot
                .folds_in_range(
                    multi_buffer::MultiBufferOffset(0)..snapshot.buffer_snapshot().len(),
                )
                .filter(|fold| fold.placeholder.type_tag.is_none())
                .count()
        });
        assert_eq!(
            persisted, 0,
            "every fold of this feature carries a type tag, so none of them is persisted"
        );
    }

    #[gpui::test]
    async fn test_code_outside_comments_is_untouched(cx: &mut TestAppContext) {
        let mut cx = markdown_comment_editor(cx).await;
        cx.set_state(indoc! {"
            let product = a * b * c;
            let text = \"**not bold**\";
            // But **this** is.
            fn mainˇ() {}
        "});
        cx.run_until_parked();
        assert_eq!(
            cx.display_text(),
            indoc! {"
                let product = a * b * c;
                let text = \"**not bold**\";
                // But this is.
                fn main() {}
            "},
            "only comments are rendered"
        );
    }

    #[gpui::test]
    async fn test_doc_comment_rows_keep_their_delimiters(cx: &mut TestAppContext) {
        let mut cx = markdown_comment_editor(cx).await;
        cx.set_state(indoc! {"
            /**
             * The **rules** of this file:
             * - A term is in `code`.
             */
            fn mainˇ() {}
        "});
        cx.run_until_parked();
        assert_eq!(
            cx.display_text(),
            indoc! {"
                /**
                 * The rules of this file:
                 * - A term is in code.
                 */
                fn main() {}
            "},
            "the delimiters stay, only the markers of the text go"
        );
    }

    #[gpui::test]
    async fn test_toggle_action_and_setting(cx: &mut TestAppContext) {
        let mut cx = markdown_comment_editor(cx).await;
        cx.set_state(indoc! {"
            // A **bold** word.
            fn mainˇ() {}
        "});
        cx.run_until_parked();
        assert_eq!(cx.display_text(), "// A bold word.\nfn main() {}\n");

        cx.update_editor(|editor, window, cx| {
            editor.toggle_markdown_comments(&ToggleMarkdownComments, window, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            cx.display_text(),
            "// A **bold** word.\nfn main() {}\n",
            "the toggle turns the rendering off for this editor"
        );

        cx.update_editor(|editor, window, cx| {
            editor.toggle_markdown_comments(&ToggleMarkdownComments, window, cx);
        });
        cx.run_until_parked();
        assert_eq!(cx.display_text(), "// A bold word.\nfn main() {}\n");

        // The editor follows the setting again once its own toggle is cleared.
        cx.update(|_, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.editor.render_markdown_in_comments = Some(false);
                });
            });
        });
        cx.update_editor(|editor, _, cx| {
            editor.markdown_comments_enabled = None;
            editor.refresh_markdown_comments(cx);
        });
        cx.run_until_parked();
        assert_eq!(
            cx.display_text(),
            "// A **bold** word.\nfn main() {}\n",
            "the setting is off, so the comment stays as its source"
        );
    }

    #[gpui::test]
    async fn test_editing_a_comment_keeps_the_buffer_correct(cx: &mut TestAppContext) {
        let mut cx = markdown_comment_editor(cx).await;
        cx.set_state(indoc! {"
            // A **bold** word.
            fn mainˇ() {}
        "});
        cx.run_until_parked();

        // Typing inside the rendered comment: the cursor moves there first, which shows the
        // source, so the edit lands where the user sees it.
        cx.set_state(indoc! {"
            // A **bold** wordˇ.
            fn main() {}
        "});
        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| {
            editor.handle_input(" here", window, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            cx.buffer_text(),
            indoc! {"
                // A **bold** word here.
                fn main() {}
            "}
        );

        cx.set_state(indoc! {"
            // A **bold** word here.
            fn mainˇ() {}
        "});
        cx.run_until_parked();
        assert_eq!(
            cx.display_text(),
            indoc! {"
                // A bold word here.
                fn main() {}
            "},
            "the edited comment renders again once the cursor leaves it"
        );
    }
}
