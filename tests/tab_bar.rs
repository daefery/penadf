//! Clicks the real tab bar (no window) and checks each click does what it shows.

use eframe::egui::{self, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use penadf::gui::tabs::{TabBarAction, TabLabel, tab_bar};

struct State {
    labels: Vec<TabLabel>,
    active: usize,
    actions: Vec<TabBarAction>,
}

fn harness() -> Harness<'static, State> {
    let labels = ["first.pdf", "second.pdf", "third.pdf"]
        .iter()
        .map(|t| TabLabel {
            title: t.to_string(),
            tooltip: String::new(),
            dirty: false,
        })
        .collect();
    Harness::builder()
        .with_size(Vec2::new(800.0, 80.0))
        .build_ui_state(
            |ui, s: &mut State| {
                if let Some(a) = tab_bar(ui, &s.labels, s.active) {
                    s.actions.push(a);
                }
            },
            State {
                labels,
                active: 1,
                actions: Vec::new(),
            },
        )
}

/// Clicks the centre of `rect` the way a mouse does: move, press, release.
fn click_at(h: &mut Harness<'_, State>, rect: egui::Rect) {
    let p = rect.center();
    h.hover_at(p);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    h.step();
    h.event(egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    h.run_steps(2);
}

#[test]
fn clicking_the_x_closes_that_tab() {
    for target in 0..3 {
        let mut h = harness();
        h.run_steps(2);
        let xs: Vec<egui::Rect> = h.query_all_by_label("×").map(|n| n.rect()).collect();
        assert_eq!(xs.len(), 3, "one close button per tab");
        click_at(&mut h, xs[target]);
        assert_eq!(
            h.state().actions,
            vec![TabBarAction::Close(target)],
            "clicked × of tab {target}"
        );
    }
}

#[test]
fn clicking_the_title_activates_that_tab() {
    let mut h = harness();
    h.run_steps(2);
    let r = h.get_by_label("third.pdf").rect();
    click_at(&mut h, r);
    assert_eq!(h.state().actions, vec![TabBarAction::Activate(2)]);
}

#[test]
fn plus_asks_for_a_new_tab() {
    let mut h = harness();
    h.run_steps(2);
    let r = h.get_by_label("+").rect();
    click_at(&mut h, r);
    assert_eq!(h.state().actions, vec![TabBarAction::OpenNew]);
}

#[test]
fn middle_click_on_a_tab_closes_it() {
    let mut h = harness();
    h.run_steps(2);
    let p = h.get_by_label("first.pdf").rect().center();
    h.hover_at(p);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Middle,
            pressed,
            modifiers: Default::default(),
        });
        h.step();
    }
    h.run_steps(2);
    assert_eq!(h.state().actions, vec![TabBarAction::Close(0)]);
}
