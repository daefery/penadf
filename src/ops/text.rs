//! Selecting and copying text on a page, the way a text editor does: press on one character,
//! drag to another, and everything between them in reading order is selected.

use super::{PagePoint, PageRect};
use crate::engine;
use anyhow::Result;
use pdfium_render::prelude::*;

/// One character of a page's text, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub ch: char,
    /// Box on the page (PDF points). Zero-sized for characters PDFium adds itself, such as the
    /// line breaks and spaces it infers between words and lines.
    pub rect: PageRect,
}

/// All characters of one page, in the order PDFium reads them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLayout {
    pub glyphs: Vec<Glyph>,
}

/// A selection on one page: character indices into its `TextLayout`, both inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub page: usize,
    pub anchor: usize,
    pub focus: usize,
}

impl Selection {
    pub fn range(&self) -> std::ops::RangeInclusive<usize> {
        self.anchor.min(self.focus)..=self.anchor.max(self.focus)
    }
}

/// Reads the character layout of `page`.
pub fn layout(doc: &PdfDocument, page: usize) -> Result<TextLayout> {
    let page = doc.pages().get(page as i32)?;
    let text = page.text()?;
    let glyphs = text
        .chars()
        .iter()
        .filter_map(|c| {
            let ch = c.unicode_char()?;
            let rect = c
                .loose_bounds()
                .map(PageRect::from_pdf)
                .unwrap_or(PageRect {
                    left: 0.0,
                    bottom: 0.0,
                    right: 0.0,
                    top: 0.0,
                });
            Some(Glyph { ch, rect })
        })
        .collect();
    Ok(TextLayout { glyphs })
}

/// Convenience for tests and tools: the layout of one page of a PDF given as bytes.
pub fn layout_of(bytes: &[u8], pw: Option<&str>, page: usize) -> Result<TextLayout> {
    layout(&engine::open(bytes.to_vec(), pw)?, page)
}

fn has_box(r: &PageRect) -> bool {
    r.right > r.left && r.top > r.bottom
}

impl TextLayout {
    pub fn is_empty(&self) -> bool {
        !self.glyphs.iter().any(|g| has_box(&g.rect))
    }

    /// The character under `p`, or the nearest one on the same line within `slack` points
    /// (so a press just left of a word, or between letters, still starts the selection).
    pub fn hit(&self, p: PagePoint, slack: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, g) in self.glyphs.iter().enumerate() {
            let r = &g.rect;
            if !has_box(r) || p.y < r.bottom - slack || p.y > r.top + slack {
                continue;
            }
            let dx = if p.x < r.left {
                r.left - p.x
            } else if p.x > r.right {
                p.x - r.right
            } else {
                0.0
            };
            let dy = if p.y < r.bottom {
                r.bottom - p.y
            } else if p.y > r.top {
                p.y - r.top
            } else {
                0.0
            };
            let d = dx + dy;
            if d <= slack && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// While dragging: the character nearest to `p` anywhere on the page, so the selection
    /// keeps following the mouse between lines and past the end of a line.
    pub fn nearest(&self, p: PagePoint) -> Option<usize> {
        self.glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| has_box(&g.rect))
            .min_by(|(_, a), (_, b)| dist(&a.rect, p).total_cmp(&dist(&b.rect, p)))
            .map(|(i, _)| i)
    }

    /// The selected text, with line breaks where the page has them.
    pub fn text(&self, sel: &Selection) -> String {
        let range = sel.range();
        let end = (*range.end()).min(self.glyphs.len().saturating_sub(1));
        let mut out: String = self.glyphs[(*range.start()).min(end)..=end]
            .iter()
            .map(|g| g.ch)
            .collect();
        // PDFium writes "\r\n" at line ends; keep plain "\n" and drop trailing breaks.
        out = out.replace("\r\n", "\n").replace('\r', "\n");
        out.trim_end_matches(['\n', ' ']).to_string()
    }

    /// Rectangles to paint for a selection: one per run of characters on the same line.
    pub fn highlight(&self, sel: &Selection) -> Vec<PageRect> {
        let mut out: Vec<PageRect> = Vec::new();
        for i in sel.range() {
            let Some(g) = self.glyphs.get(i) else { break };
            let r = g.rect;
            if !has_box(&r) {
                continue;
            }
            match out.last_mut() {
                Some(last) if same_line(last, &r) && r.left >= last.left - 1.0 => {
                    last.right = last.right.max(r.right);
                    last.top = last.top.max(r.top);
                    last.bottom = last.bottom.min(r.bottom);
                }
                _ => out.push(r),
            }
        }
        out
    }

    /// Everything on the page (Ctrl+A).
    pub fn select_all(&self, page: usize) -> Option<Selection> {
        let last = self.glyphs.len().checked_sub(1)?;
        Some(Selection {
            page,
            anchor: 0,
            focus: last,
        })
    }

    /// The word around character `i` (double-click).
    pub fn word_at(&self, page: usize, i: usize) -> Option<Selection> {
        let is_word = |c: char| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ',';
        if !self.glyphs.get(i).is_some_and(|g| is_word(g.ch)) {
            return None;
        }
        let mut a = i;
        while a > 0 && is_word(self.glyphs[a - 1].ch) {
            a -= 1;
        }
        let mut b = i;
        while b + 1 < self.glyphs.len() && is_word(self.glyphs[b + 1].ch) {
            b += 1;
        }
        // A trailing full stop or comma ends a sentence; do not include it.
        while b > a && matches!(self.glyphs[b].ch, '.' | ',') {
            b -= 1;
        }
        Some(Selection {
            page,
            anchor: a,
            focus: b,
        })
    }
}

fn dist(r: &PageRect, p: PagePoint) -> f32 {
    let dx = (r.left - p.x).max(0.0).max(p.x - r.right);
    let dy = (r.bottom - p.y).max(0.0).max(p.y - r.top);
    // Lines matter more than columns when choosing between rows.
    dx + 3.0 * dy
}

fn same_line(a: &PageRect, b: &PageRect) -> bool {
    let ca = (a.bottom + a.top) / 2.0;
    let cb = (b.bottom + b.top) / 2.0;
    (ca - cb).abs() <= (a.top - a.bottom).max(b.top - b.bottom) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "ab cd\r\nef" laid out like a page: two lines, 10 pt per letter, line 2 lower.
    fn sample() -> TextLayout {
        let mut glyphs = Vec::new();
        let mut put = |ch, x: f32, y: f32| {
            glyphs.push(Glyph {
                ch,
                rect: PageRect {
                    left: x,
                    bottom: y,
                    right: x + 8.0,
                    top: y + 10.0,
                },
            })
        };
        put('a', 0.0, 100.0);
        put('b', 10.0, 100.0);
        put(' ', 20.0, 100.0);
        put('c', 30.0, 100.0);
        put('d', 40.0, 100.0);
        let zero = PageRect {
            left: 0.0,
            bottom: 0.0,
            right: 0.0,
            top: 0.0,
        };
        glyphs.push(Glyph {
            ch: '\r',
            rect: zero,
        });
        glyphs.push(Glyph {
            ch: '\n',
            rect: zero,
        });
        let mut put2 = |ch, x: f32| {
            glyphs.push(Glyph {
                ch,
                rect: PageRect {
                    left: x,
                    bottom: 80.0,
                    right: x + 8.0,
                    top: 90.0,
                },
            })
        };
        put2('e', 0.0);
        put2('f', 10.0);
        TextLayout { glyphs }
    }

    #[test]
    fn selection_across_lines_copies_with_a_line_break() {
        let l = sample();
        let sel = Selection {
            page: 0,
            anchor: 3,
            focus: 8,
        };
        assert_eq!(l.text(&sel), "cd\nef");
        // Dragging backwards selects the same text.
        assert_eq!(
            l.text(&Selection {
                page: 0,
                anchor: 8,
                focus: 3
            }),
            "cd\nef"
        );
    }

    #[test]
    fn highlight_is_one_rect_per_line() {
        let l = sample();
        let rects = l.highlight(&Selection {
            page: 0,
            anchor: 1,
            focus: 8,
        });
        assert_eq!(rects.len(), 2);
        assert_eq!((rects[0].left, rects[0].right), (10.0, 48.0));
        assert_eq!((rects[1].left, rects[1].right), (0.0, 18.0));
    }

    #[test]
    fn hit_finds_the_character_or_a_near_one_on_the_same_line() {
        let l = sample();
        assert_eq!(l.hit(PagePoint { x: 34.0, y: 105.0 }, 3.0), Some(3));
        assert_eq!(
            l.hit(PagePoint { x: -2.0, y: 105.0 }, 3.0),
            Some(0),
            "just left of the line"
        );
        assert_eq!(
            l.hit(PagePoint { x: 34.0, y: 50.0 }, 3.0),
            None,
            "empty space below the text"
        );
        // Dragging past the end of a line still selects up to its last character.
        assert_eq!(l.nearest(PagePoint { x: 200.0, y: 104.0 }), Some(4));
    }

    #[test]
    fn word_and_select_all() {
        let l = sample();
        assert_eq!(
            l.word_at(0, 4),
            Some(Selection {
                page: 0,
                anchor: 3,
                focus: 4
            })
        );
        assert_eq!(
            l.word_at(0, 2),
            None,
            "double-click on a space selects nothing"
        );
        let all = l.select_all(0).unwrap();
        assert_eq!(l.text(&all), "ab cd\nef");
        assert!(TextLayout::default().select_all(0).is_none());
        assert!(TextLayout::default().is_empty());
    }
}
