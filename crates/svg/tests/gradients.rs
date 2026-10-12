//! Gradient defs: text gradients span the laid-out text, stroke gradients the stroked area,
//! identical gradients share a def named after their swatch, and midpoints survive re-import.
// Integration tests: unwrapping and panicking on failure is fine here, unlike in shipped code (AGENTS.md › Robustness).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use vectorcraft_color::{Color, Gradient, GradientKind, GradientPaint, GradientStop, Paint};
use vectorcraft_doc::{Appearance, CharStyle, Document, Node, NodeKind, TextObject};
use vectorcraft_geom::{Point, Rect, shapes};
use vectorcraft_svg::{ExportOptions, export, import};

fn black_to_white() -> GradientPaint {
    GradientPaint::new(Gradient {
        kind: GradientKind::Linear,
        stops: vec![GradientStop::new(0.0, Color::BLACK), GradientStop::new(1.0, Color::WHITE)],
    })
}

fn doc_with(nodes: impl FnOnce(&mut Document) -> Vec<Node>) -> Document {
    let mut d = Document::new(300.0, 200.0);
    let l = d.layers[0].id;
    for n in nodes(&mut d) {
        d.insert(Some(l), usize::MAX, n).unwrap();
    }
    d
}

/// The value of `name` on the first element whose tag starts with `tag`.
fn attr(svg: &str, tag: &str, name: &str) -> f64 {
    let el = &svg[svg.find(tag).unwrap_or_else(|| panic!("no {tag}: {svg}"))..];
    let el = &el[..el.find('>').unwrap()];
    let k = format!(" {name}=\"");
    let i = el.find(&k).unwrap_or_else(|| panic!("no {name} in {el}")) + k.len();
    el[i..i + el[i..].find('"').unwrap()].parse().unwrap()
}

#[test]
fn gradient_text_spans_its_layout() {
    let st = CharStyle { size: 24.0, fill: Paint::Gradient(Box::new(black_to_white())), ..CharStyle::default() };
    let t = TextObject::point(Point::new(30.0, 100.0), "Gradient", st);
    let bounds = vectorcraft_text::layout(vectorcraft_text::FontDb::global(), &t).bounds;
    let d = doc_with(|d| vec![Node::new(d.alloc_id(), NodeKind::Text(Box::new(t)))]);
    let svg = export(&d, &ExportOptions::default());
    assert!(svg.contains("fill=\"url(#linear-gradient-1)\""), "{svg}");
    // Text space is the <text> element's user space: the gradient runs across the glyphs.
    assert!((attr(&svg, "<linearGradient", "x1") - bounds.x0).abs() < 0.01, "{svg}");
    assert!((attr(&svg, "<linearGradient", "x2") - bounds.x1).abs() < 0.01, "{svg}");
    // Outlined, the same gradient sits on the outlines.
    let outlined = export(&d, &ExportOptions { outline_text: true, ..Default::default() });
    assert!((attr(&outlined, "<linearGradient", "x1") - (30.0 + bounds.x0)).abs() < 0.01, "{outlined}");
}

#[test]
fn stroke_gradients_span_the_stroke() {
    let d = doc_with(|d| {
        vec![Node::path(
            d.alloc_id(),
            shapes::rectangle(Rect::new(10.0, 20.0, 110.0, 70.0)),
            Appearance::basic(Paint::None, Paint::Gradient(Box::new(black_to_white())), 10.0),
        )]
    });
    let svg = export(&d, &ExportOptions::default());
    assert!(svg.contains("stroke=\"url(#"), "{svg}");
    assert_eq!(attr(&svg, "<linearGradient", "x1"), 5.0, "bounds less half the width");
    assert_eq!(attr(&svg, "<linearGradient", "x2"), 115.0);
}

#[test]
fn identical_gradients_share_a_def_named_after_the_swatch() {
    let mut g = black_to_white();
    g.swatch = Some("Night Fade".into());
    let fill = Appearance::basic(Paint::Gradient(Box::new(g.clone())), Paint::None, 0.0);
    let d = doc_with(|d| {
        let mut nodes = vec![];
        for _ in 0..3 {
            nodes.push(Node::path(d.alloc_id(), shapes::rectangle(Rect::new(10.0, 10.0, 60.0, 60.0)), fill.clone()));
        }
        // Elsewhere, the gradient fits other bounds: a def of its own.
        nodes.push(Node::path(d.alloc_id(), shapes::rectangle(Rect::new(100.0, 10.0, 200.0, 60.0)), fill.clone()));
        nodes
    });
    let svg = export(&d, &ExportOptions::default());
    assert_eq!(svg.matches("<linearGradient").count(), 2, "{svg}");
    assert_eq!(svg.matches("url(#Night_Fade)").count(), 3, "{svg}");
    assert!(svg.contains("id=\"Night_Fade-2\""), "{svg}");
}

#[test]
fn midpoints_come_back() {
    let mut g = black_to_white();
    g.gradient.stops[0].midpoint = 0.25;
    let d = doc_with(|d| {
        vec![Node::path(
            d.alloc_id(),
            shapes::rectangle(Rect::new(0.0, 0.0, 100.0, 100.0)),
            Appearance::basic(Paint::Gradient(Box::new(g.clone())), Paint::None, 0.0),
        )]
    });
    let svg = export(&d, &ExportOptions::default());
    // The midpoint's curve as stops for other viewers, each marked as standing for it.
    let stops = svg.matches("<stop ").count();
    assert!(stops > 3, "{svg}");
    assert_eq!(svg.matches("data-vc-midpoint=\"0.25\"").count(), stops - 2, "{svg}");
    // GUARDS: the stops draw the curve `Gradient::sample` draws, half-way grey at the midpoint; 1.5 levels allow the hex rounding.
    let written: Vec<(f32, f32)> = svg
        .split("<stop ")
        .skip(1)
        .map(|el| {
            let value = |name: &str| el.split(&format!("{name}=\"")).nth(1).unwrap().split('"').next().unwrap();
            (value("offset").parse().unwrap(), Color::from_hex(value("stop-color")).unwrap().to_rgb()[0])
        })
        .collect();
    let drawn = |t: f32| {
        let w = written.windows(2).find(|w| t <= w[1].0).unwrap();
        w[0].1 + (w[1].1 - w[0].1) * (t - w[0].0) / (w[1].0 - w[0].0)
    };
    assert!((drawn(0.25) - 0.5).abs() < 1.5 / 255.0, "{written:?}");
    for i in 0..=100 {
        let t = i as f32 / 100.0;
        assert!((drawn(t) - g.gradient.sample(t).0.to_rgb()[0]).abs() < 1.5 / 255.0, "at {t}: {written:?}");
    }
    let back = import(&svg).unwrap();
    let n = &back.layers[0].children().unwrap()[0];
    let Paint::Gradient(g) = n.appearance.fill_paint() else { panic!("{svg}") };
    assert_eq!(g.gradient.stops.len(), 2);
    assert!((g.gradient.stops[0].midpoint - 0.25).abs() < 1e-6);
    assert!((g.gradient.stops[1].midpoint - 0.5).abs() < 1e-6);
}
