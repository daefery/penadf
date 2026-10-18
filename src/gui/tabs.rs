//! Tabs: several open documents in one window.
//!
//! The active tab's state lives outside the strip (in the app, as plain fields), so UI code
//! reads and writes it directly. The strip keeps the other tabs parked, with an empty slot
//! where the active one belongs. Switching swaps the active state into its slot and takes
//! the new one out.

#[derive(Debug)]
pub struct TabStrip<T> {
    slots: Vec<Option<T>>,
    active: usize,
}

impl<T> Default for TabStrip<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            active: 0,
        }
    }
}

impl<T: Default> TabStrip<T> {
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn active(&self) -> usize {
        self.active
    }

    /// Adds `new` right after the active tab and makes it active.
    pub fn open(&mut self, cur: &mut T, new: T) {
        if self.slots.is_empty() {
            self.slots.push(None);
            self.active = 0;
        } else {
            self.slots[self.active] = Some(std::mem::take(cur));
            self.active += 1;
            self.slots.insert(self.active, None);
        }
        *cur = new;
    }

    /// Makes tab `index` active. Out-of-range or already-active indices do nothing.
    pub fn activate(&mut self, cur: &mut T, index: usize) {
        if index == self.active || index >= self.slots.len() {
            return;
        }
        self.slots[self.active] = Some(std::mem::take(cur));
        *cur = self.slots[index].take().expect("parked tab");
        self.active = index;
    }

    /// Next / previous tab, wrapping around.
    pub fn cycle(&mut self, cur: &mut T, forward: bool) {
        let n = self.slots.len();
        if n > 1 {
            let next = if forward {
                (self.active + 1) % n
            } else {
                (self.active + n - 1) % n
            };
            self.activate(cur, next);
        }
    }

    /// Closes the active tab and returns its state. The tab to its right becomes active
    /// (or the new last tab). With no tabs left, `cur` is the default (empty) state.
    pub fn close_active(&mut self, cur: &mut T) -> T {
        let closed = std::mem::take(cur);
        if self.slots.is_empty() {
            return closed;
        }
        self.slots.remove(self.active);
        if !self.slots.is_empty() {
            self.active = self.active.min(self.slots.len() - 1);
            *cur = self.slots[self.active].take().expect("parked tab");
        } else {
            self.active = 0;
        }
        closed
    }

    /// The state of tab `index`, given the active state.
    pub fn get<'a>(&'a self, cur: &'a T, index: usize) -> Option<&'a T> {
        if index == self.active && index < self.slots.len() {
            Some(cur)
        } else {
            self.slots.get(index)?.as_ref()
        }
    }

    /// Mutable access to a parked tab (not the active one).
    pub fn parked_mut(&mut self, index: usize) -> Option<&mut T> {
        self.slots.get_mut(index)?.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(s: &TabStrip<String>, cur: &String) -> Vec<String> {
        (0..s.len())
            .map(|i| s.get(cur, i).unwrap().clone())
            .collect()
    }

    #[test]
    fn open_inserts_after_active_and_activates() {
        let (mut s, mut cur) = (TabStrip::default(), String::new());
        s.open(&mut cur, "a".into());
        s.open(&mut cur, "b".into());
        s.activate(&mut cur, 0);
        s.open(&mut cur, "c".into());
        assert_eq!(names(&s, &cur), ["a", "c", "b"]);
        assert_eq!((s.active(), cur.as_str()), (1, "c"));
    }

    #[test]
    fn activate_and_cycle() {
        let (mut s, mut cur) = (TabStrip::default(), String::new());
        for n in ["a", "b", "c"] {
            s.open(&mut cur, n.into());
        }
        s.activate(&mut cur, 0);
        assert_eq!(cur, "a");
        s.cycle(&mut cur, false);
        assert_eq!(cur, "c");
        s.cycle(&mut cur, true);
        assert_eq!(cur, "a");
        s.activate(&mut cur, 9); // ignored
        assert_eq!((s.active(), cur.as_str()), (0, "a"));
        assert_eq!(names(&s, &cur), ["a", "b", "c"]);
    }

    #[test]
    fn close_picks_right_neighbour_then_last_then_empty() {
        let (mut s, mut cur) = (TabStrip::default(), String::new());
        for n in ["a", "b", "c"] {
            s.open(&mut cur, n.into());
        }
        s.activate(&mut cur, 1);
        assert_eq!(s.close_active(&mut cur), "b");
        assert_eq!((s.active(), cur.as_str()), (1, "c"));
        assert_eq!(s.close_active(&mut cur), "c");
        assert_eq!((s.active(), cur.as_str()), (0, "a"));
        assert_eq!(s.close_active(&mut cur), "a");
        assert!(s.is_empty() && cur.is_empty());
        assert_eq!(
            s.close_active(&mut cur),
            "",
            "closing with no tabs is harmless"
        );
    }

    #[test]
    fn parked_tabs_are_editable_but_active_is_not_parked() {
        let (mut s, mut cur) = (TabStrip::default(), String::new());
        s.open(&mut cur, "a".into());
        s.open(&mut cur, "b".into());
        s.parked_mut(0).unwrap().push('!');
        assert!(s.parked_mut(1).is_none());
        s.activate(&mut cur, 0);
        assert_eq!(cur, "a!");
    }
}

// ---------- the tab bar widget ----------

use eframe::egui::{self, Color32, RichText};

/// What one tab shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabLabel {
    pub title: String,
    /// Shown on hover (the file's full path).
    pub tooltip: String,
    pub dirty: bool,
}

/// What the user did in the tab bar this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabBarAction {
    Activate(usize),
    Close(usize),
    OpenNew,
}

/// Draws the tabs and returns the user's click, if any.
pub fn tab_bar(ui: &mut egui::Ui, labels: &[TabLabel], active: usize) -> Option<TabBarAction> {
    let mut action = None;
    egui::ScrollArea::horizontal()
        .id_salt("tab-scroll")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, tab) in labels.iter().enumerate() {
                    if let Some(a) = one_tab(ui, i, tab, i == active) {
                        action = Some(a);
                    }
                }
                if ui
                    .add(egui::Button::new(RichText::new("+").size(17.0)).frame(false))
                    .on_hover_text("Open a file in a new tab (Ctrl+O)")
                    .clicked()
                {
                    action = Some(TabBarAction::OpenNew);
                }
            });
        });
    action
}

fn one_tab(ui: &mut egui::Ui, i: usize, tab: &TabLabel, selected: bool) -> Option<TabBarAction> {
    let frame = egui::Frame::new()
        .fill(if selected {
            Color32::WHITE
        } else {
            Color32::from_rgb(220, 222, 226)
        })
        .corner_radius(egui::CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        })
        .inner_margin(egui::Margin {
            left: 10,
            right: 4,
            top: 4,
            bottom: 4,
        });
    // The tab body senses clicks *underneath* its title and × (egui gives a click to the
    // widget registered last, so a body registered after the × would steal its clicks).
    let body = egui::UiBuilder::new()
        .id_salt(("tab", i))
        .sense(egui::Sense::click());
    let shown = ui.scope_builder(body, |ui| {
        frame
            .show(ui, |ui| {
                ui.set_max_width(220.0);
                ui.horizontal(|ui| {
                    let title = if tab.dirty {
                        format!("• {}", tab.title)
                    } else {
                        tab.title.clone()
                    };
                    let text = if selected {
                        RichText::new(title).strong()
                    } else {
                        RichText::new(title)
                    };
                    // The title does not sense clicks itself; the body under it does.
                    ui.add(egui::Label::new(text).truncate().selectable(false));
                    ui.add(egui::Button::new(RichText::new("×").size(15.0)).frame(false))
                        .on_hover_text("Close tab (Ctrl+W)")
                })
                .inner
            })
            .inner
    });
    let close = shown.inner;
    let whole = shown.response.on_hover_text(&tab.tooltip);
    if close.clicked() || whole.middle_clicked() {
        Some(TabBarAction::Close(i)) // middle-click closes, as in browsers
    } else if whole.clicked() {
        Some(TabBarAction::Activate(i))
    } else {
        None
    }
}
