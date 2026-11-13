//! Regression check on the native GPUI text backend. Opens a temporary window,
//! measures each text node repeatedly, then exits without touching the library.
use gpui::{
    App, AppContext, Application, AvailableSpace, Context, Element, IntoElement, ParentElement,
    Render, Styled, StyledText, TextOverflow, TextStyleRefinement, Window, WindowOptions, canvas,
    div, px, size,
};

struct TextLayoutSmoke;

impl Render for TextLayoutSmoke {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            canvas(
                |_, window, cx| {
                    let style = TextStyleRefinement {
                        font_family: Some("Segoe UI".into()),
                        text_overflow: Some(TextOverflow::Truncate("…".into())),
                        ..Default::default()
                    };
                    window.with_text_style(Some(style), |window| {
                        for text in [
                            "найтивыход - спектрофобия",
                            "Неизвестный исполнитель — длинное название трека",
                            "Long ASCII track title for repeated measurement",
                            "Музыка 🎵 日本語 — смешанное название",
                            "Первая строка\nВторая строка с длинным названием",
                        ] {
                            // A single layout's measure callback is reused by Taffy.
                            // Narrowing adds a 3-byte ellipsis; reusing the truncated
                            // runs with the original Cyrillic text splits a character.
                            let mut reused = StyledText::new(text);
                            let (id, _) = reused.request_layout(None, None, window, cx);
                            for width in [160., 110., 65., 280., 900., 45., 160.] {
                                let space = size(
                                    AvailableSpace::Definite(px(width)),
                                    AvailableSpace::MinContent,
                                );
                                window.compute_layout(id, space, cx);
                                let measured = reused.layout().text();
                                let mut fresh = StyledText::new(text);
                                let (fresh_id, _) =
                                    fresh.request_layout(None, None, window, cx);
                                window.compute_layout(fresh_id, space, cx);
                                assert_eq!(measured, fresh.layout().text());
                                assert_eq!(
                                    reused.layout().len(),
                                    fresh.layout().len(),
                                    "repeated measurement changed text length"
                                );
                            }
                        }
                    });
                    println!("Text layout smoke: repeated Cyrillic, ASCII, emoji, CJK and multiline measurements passed.");
                    cx.quit();
                },
                |_, _, _, _| {},
            )
            .size_full(),
        )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|_| TextLayoutSmoke)
        })
        .expect("open text regression window");
    });
}
