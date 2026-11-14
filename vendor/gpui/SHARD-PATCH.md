# Shard GPUI patch

Source: crates.io `gpui` 0.2.2, licensed under Apache-2.0 (see
`LICENSE-APACHE`). Cargo selects this copy through `[patch.crates-io]`.

The only Rust source modification is in `src/elements/text.rs`,
`TextLayout::layout`: keep the original text runs immutable and clone them
inside each measured-layout callback before `truncate_line` mutates them.

Taffy measures the same text node at multiple widths. Previously, a second
measurement combined the original text with already truncated run lengths.
The UTF-8 ellipsis can leave a run boundary inside a Cyrillic character,
panicking in the Windows DirectWrite backend. In the desktop event callback
this can terminate the application with `0xc0000409`.

Regression: `cargo run --example text_layout_smoke` measures the same layout
at narrow, wide and narrow widths again, and compares it with fresh layouts.
It covers Cyrillic, ASCII, emoji, CJK and multiline text. The unpatched
0.2.2 fails this check on Windows with a UTF-8 boundary panic.

Keep this directory when sharing/building Shard. Remove the Cargo patch only
after an upstream version passes the regression and playback UI checks.
