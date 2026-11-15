# Shard GPUI patch

Source: crates.io `gpui` 0.2.2, licensed under Apache-2.0 (see
`LICENSE-APACHE`). Cargo selects this copy through `[patch.crates-io]`.

`src/elements/text.rs`, `TextLayout::layout`: keep the original text runs immutable and clone them
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

`src/window.rs`, `Window::bounds_changed`: notify bounds observers for moves as
before, but only invalidate the scene if the content size, scale factor or display
changes. On Windows every `WM_MOVE` previously rebuilt the full view and layout
while the compositor was moving an otherwise unchanged window.

Regression: `cargo run --locked --example window_move_smoke` moves a native window
100 times, verifies that bounds observers still receive those events and that the
scene is not rebuilt per move, then resizes it and verifies layout invalidation
and the updated viewport.

Keep this directory when sharing/building Shard. Remove the Cargo patch only
after an upstream version passes the regression and playback UI checks.
