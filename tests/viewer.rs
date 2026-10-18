//! Drives the page view with simulated mouse input (no window) and checks that what
//! the user does on screen becomes the right action at the right page position.

use eframe::egui::{self, Pos2, Vec2};
use egui_kittest::Harness;
use penadf::gui::document::Document;
use penadf::gui::viewer::{PageAction, Tool, Viewer};
use penadf::ops::{self, create, pages};

struct State {
    doc: Document,
    viewer: Viewer,
    actions: Vec<PageAction>,
    /// Screen rect of the central area, for converting page fractions to screen points.
    area: egui::Rect,
}

fn one_page_doc(rotate: i32) -> Document {
    let pdf = create::from_text("Hello viewer\nsecond line").unwrap();
    let pdf = pages::rotate(&pdf, None, &[0], rotate).unwrap();
    Document::open(None, "t.pdf".into(), pdf, None).unwrap()
}

fn harness(doc: Document, tool: Tool) -> Harness<'static, State> {
    let mut viewer = Viewer::default();
    viewer.tool = tool;
    let state = State {
        doc,
        viewer,
        actions: Vec::new(),
        area: egui::Rect::NOTHING,
    };
    let mut h = Harness::builder()
        .with_size(Vec2::new(800.0, 900.0))
        .build_ui_state(
            |ui, s: &mut State| {
                s.area = ui.max_rect();
                if let Some(a) = s.viewer.show(ui, &mut s.doc, &[], None) {
                    s.actions.push(a);
                }
            },
            state,
        );
    h.run_steps(3); // let fit-to-width settle
    h
}

/// Screen position of a point given as fractions of the (single) displayed page.
fn page_pos(s: &State, u: f32, v: f32) -> Pos2 {
    let (w, h) = s.doc.geoms[0].display_size();
    let size = Vec2::new(w, h) * s.viewer.zoom;
    let gap = 16.0;
    let left = s.area.center().x - size.x / 2.0;
    let top = s.area.top() + gap;
    Pos2::new(left + u * size.x, top + v * size.y)
}

fn drag(h: &mut Harness<'_, State>, from: Pos2, to: Pos2) {
    h.hover_at(from);
    h.step();
    h.drag_at(from);
    h.step();
    for i in 1..=5 {
        h.hover_at(from + (to - from) * (i as f32 / 5.0));
        h.step();
    }
    h.drop_at(to);
    h.step();
    h.step();
}

#[test]
fn dragging_a_box_reports_page_coordinates() {
    for rotate in [0, 1] {
        let mut h = harness(one_page_doc(rotate), Tool::Highlight);
        let (a, b) = (page_pos(h.state(), 0.1, 0.1), page_pos(h.state(), 0.5, 0.3));
        drag(&mut h, a, b);
        let s = h.state();
        assert_eq!(s.actions.len(), 1, "rotate {rotate}: {:?}", s.actions);
        let PageAction::Box { page, tool, rect } = s.actions[0].clone() else {
            panic!("not a box: {:?}", s.actions[0])
        };
        assert_eq!((page, tool), (0, Tool::Highlight));
        let g = s.doc.geoms[0];
        let want = ops::PageRect::from_corners(g.to_page(0.1, 0.1), g.to_page(0.5, 0.3));
        for (got, exp) in [
            (rect.left, want.left),
            (rect.bottom, want.bottom),
            (rect.right, want.right),
            (rect.top, want.top),
        ] {
            assert!(
                (got - exp).abs() < 3.0,
                "rotate {rotate}: got {rect:?}, want {want:?}"
            );
        }
    }
}

#[test]
fn clicking_with_note_tool_reports_the_spot() {
    let mut h = harness(one_page_doc(0), Tool::Note);
    let p = page_pos(h.state(), 0.25, 0.5);
    h.hover_at(p);
    h.step();
    h.drag_at(p);
    h.step();
    h.drop_at(p);
    h.run_steps(2);
    let s = h.state();
    let Some(PageAction::Click { at, tool, .. }) = s.actions.first().cloned() else {
        panic!("{:?}", s.actions)
    };
    assert_eq!(tool, Tool::Note);
    let want = s.doc.geoms[0].to_page(0.25, 0.5);
    assert!(
        (at.x - want.x).abs() < 3.0 && (at.y - want.y).abs() < 3.0,
        "{at:?} vs {want:?}"
    );
}

#[test]
fn drawing_collects_ink_until_taken() {
    let mut h = harness(one_page_doc(0), Tool::Draw);
    let (a, b) = (
        page_pos(h.state(), 0.2, 0.2),
        page_pos(h.state(), 0.4, 0.25),
    );
    drag(&mut h, a, b);
    assert!(
        h.state().actions.is_empty(),
        "ink is kept until the drawing is finished"
    );
    assert!(h.state().viewer.has_ink());
    let done = h.state_mut().viewer.take_ink();
    let Some(PageAction::InkDone { page, strokes }) = done else {
        panic!("{done:?}")
    };
    assert_eq!(page, 0);
    assert_eq!(strokes.len(), 1);
    assert!(
        strokes[0].len() >= 3,
        "stroke has {} points",
        strokes[0].len()
    );
}

#[test]
fn select_tool_does_nothing_on_click() {
    let mut h = harness(one_page_doc(0), Tool::Select);
    let p = page_pos(h.state(), 0.5, 0.5);
    drag(&mut h, p, p + Vec2::new(40.0, 40.0));
    assert!(h.state().actions.is_empty());
}
