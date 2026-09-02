# Local features

Everything this fork adds on top of upstream Zed. The `integration` branch carries all of it and
is what the daily Windows build is made from. Each feature also lives on its own branch, cut from
a recent upstream `main`, which is what a pull request uses.

Upstream caps open pull requests at three per author, so features queue here until a slot frees up.

| Feature | What it does | Setting / binding | Branch | Fork PR | Upstream PR |
| --- | --- | --- | --- | --- | --- |
| Directory diff stats | Cumulative +/− line counts next to directories in the git panel tree view | `git_panel.directory_diff_stats` (off) | `git-panel-directory-diff-stats` | [#1](https://github.com/alpharder/zed/pull/1) | [#62967](https://github.com/zed-industries/zed/pull/62967) — open |
| Expand / collapse all | One state-aware button in the git panel header, plus context menu entries and key bindings | `git_panel::ExpandAllEntries` / `CollapseAllEntries`, `ctrl-right` / `ctrl-left` | `git-panel-expand-collapse-all` | [#2](https://github.com/alpharder/zed/pull/2) | — |
| Sticky directories | Ancestor directories stay pinned while the git panel tree scrolls | `git_panel.sticky_scroll` (off) | `git-panel-sticky-scroll` | [#3](https://github.com/alpharder/zed/pull/3) | — |
| Preview tabs from the git panel | Entries opened from the git panel reuse the preview tab | `preview_tabs.enable_preview_from_git_panel` (on) | `git-panel-preview-tabs` | [#4](https://github.com/alpharder/zed/pull/4) | [#62915](https://github.com/zed-industries/zed/pull/62915) — open |
| Status filter | Funnel button that filters the changes list by added / modified / deleted, per session | git panel header button | `git-panel-status-filter` | [#5](https://github.com/alpharder/zed/pull/5) | — |
| Sort by lines changed | Third sort mode in the git panel list view | `git_panel.sort_by: "lines_changed"` | **none yet** (only on `integration`) | — | — |
| Markdown highlight paint order | Search and selection highlights no longer cover the preview text | — | `markdown-highlight-paint-order` | [#6](https://github.com/alpharder/zed/pull/6) | [#62963](https://github.com/zed-industries/zed/pull/62963) — open |
| Outline modal in the markdown preview | `ctrl-shift-o` works in the preview, the preview follows outline navigation, and a single click in the outline panel scrolls it | `ctrl-shift-o` in the `MarkdownPreview` context | **none yet** (only on `integration`) | — | — |
| Outline revamp | Symbol kinds as icons instead of keywords, `hidden_outline_symbols` to drop local noise, font size and line height for the panel and the modal | `languages.*.hidden_outline_symbols` (`["local"]`), `outline_panel.font_size`, `outline_panel.line_height` | `outline-revamp` | [#7](https://github.com/alpharder/zed/pull/7) | — |
| Reveal in project panel button | Tab bar button next to the navigation buttons that reveals the active file in the project panel | `tab_bar.show_reveal_in_project_panel_button` (off) | `tab-bar-reveal-button` | — | — |
| Editor scrollbar width | The width of the editor scrollbar is configurable | `scrollbar.width` (15) | `editor-scrollbar-width` | — | — |
| Markdown in comments | The Markdown of a comment is rendered in place: the markers are folded away, the text they mark is styled, and the comment holding the cursor stays as its source | `render_markdown_in_comments` (off), Editor Controls → Markdown Comments, `editor::ToggleMarkdownComments` | `editor-markdown-comments` | — | — |

## Notes

- Features without a branch need one cut from upstream `main` before they can become a pull
  request; they exist only as commits on `integration`.
- Upstream wants a discussion before a pull request for anything that is not a small enhancement.
  The outline revamp has one to point at: [discussion #49219](https://github.com/zed-industries/zed/discussions/49219),
  plus [maxbrunsfeld's comment](https://github.com/zed-industries/zed/pull/39797#issuecomment-3641981249)
  on the pull request that introduced the noise.
- `README.md` on every feature branch keeps the `> [!IMPORTANT]` review marker that upstream's
  `.rules` asks for. Remove it by hand after reviewing the diff, before submitting.
- The daily Windows build: `zed-win-build release`, then reinstall the matching dev remote server
  (`cargo build -p remote_server --features debug-embed`, copy to `~/.zed_server/`).
