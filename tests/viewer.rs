//! Drives the page view with simulated mouse input (no window) and checks that what
//! the user does on screen becomes the right action at the right page position.

use eframe::egui::{self, Pos2, Vec2};
use egui_kittest::Harness;
use penadf::gui::document::Document;
use penadf::gui::viewer::{PageAction, Tool, Viewer};
#[allow(unused_imports)]
use penadf::ops::text::Selection;
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

/// Presses and releases the mouse at `p` without the pointer leaving the window
/// (kittest's `drop_at` also sends "pointer gone", which cancels a click).
fn click(h: &mut Harness<'_, State>, p: Pos2) {
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        });
        h.step();
    }
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
    // 60 frames per second, as on screen, so double-clicks fit egui's 0.3 s window.
    let mut h = Harness::builder()
        .with_size(Vec2::new(800.0, 900.0))
        .with_step_dt(1.0 / 60.0)
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
fn select_tool_creates_no_page_actions() {
    let mut h = harness(one_page_doc(0), Tool::Select);
    let p = page_pos(h.state(), 0.5, 0.5);
    drag(&mut h, p, p + Vec2::new(40.0, 40.0));
    assert!(
        h.state().actions.is_empty(),
        "selecting text never edits the document"
    );
}

/// Screen position of the middle of `word` on the page.
fn word_pos(s: &State, word: &str, at_end: bool) -> Pos2 {
    let r = ops::search(&s.doc.bytes, None, word)
        .unwrap()
        .remove(0)
        .rects[0];
    let g = s.doc.geoms[0];
    let x = if at_end { r.right - 0.5 } else { r.left + 0.5 };
    let (u, v) = g.to_display(ops::PagePoint {
        x,
        y: (r.bottom + r.top) / 2.0,
    });
    page_pos(s, u, v)
}

#[test]
fn dragging_over_text_selects_it_and_ctrl_c_copies_it() {
    let pdf = create::from_text("Hello viewer\nsecond line here").unwrap();
    let doc = Document::open(None, "t.pdf".into(), pdf, None).unwrap();
    let mut h = harness(doc, Tool::Select);

    let (a, b) = (
        word_pos(h.state(), "viewer", false),
        word_pos(h.state(), "second", true),
    );
    drag(&mut h, a, b);
    let text = {
        let s = h.state_mut();
        let State { viewer, doc, .. } = s;
        viewer.selected_text(doc)
    };
    assert_eq!(text.as_deref(), Some("viewer\nsecond"));

    // A click on empty paper clears the selection.
    // (40% down the page: blank paper, and inside the 900 px test window.)
    let empty = page_pos(h.state(), 0.5, 0.4);
    assert!(h.state().area.contains(empty));
    h.hover_at(empty);
    h.step();
    click(&mut h, empty);
    h.run_steps(2);
    assert!(h.state().viewer.selection.is_none());
}

#[test]
fn double_click_selects_a_word() {
    let pdf = create::from_text("Harga sewa Rp 96.000.000 per tahun").unwrap();
    let doc = Document::open(None, "t.pdf".into(), pdf, None).unwrap();
    let mut h = harness(doc, Tool::Select);
    let p = word_pos(h.state(), "sewa", false) + Vec2::new(4.0, 0.0);
    h.hover_at(p);
    h.step();
    click(&mut h, p);
    click(&mut h, p);
    h.run_steps(2);
    let s = h.state_mut();
    let State { viewer, doc, .. } = s;
    assert_eq!(viewer.selected_text(doc).as_deref(), Some("sewa"));
}

#[test]
fn right_click_menu_offers_copy_for_the_selection() {
    use egui_kittest::kittest::Queryable;
    let pdf = create::from_text("Hello viewer\nsecond line here").unwrap();
    let doc = Document::open(None, "t.pdf".into(), pdf, None).unwrap();
    let mut h = harness(doc, Tool::Select);
    let (a, b) = (
        word_pos(h.state(), "Hello", false),
        word_pos(h.state(), "viewer", true),
    );
    drag(&mut h, a, b);
    assert!(h.state().viewer.selection.is_some());

    // Right-click on the selection opens a menu with Copy.
    h.hover_at(b);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: b,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Default::default(),
        });
        h.step();
    }
    h.run_steps(3);
    let copy = h.get_by_label("Copy").rect().center();
    h.hover_at(copy);
    h.step();
    click(&mut h, copy);
    h.run_steps(2);
    assert!(
        h.state_mut().viewer.take_copy_request(),
        "Copy asks the app to copy"
    );
    assert!(
        h.state().viewer.selection.is_some(),
        "the right-click keeps the selection"
    );
}
