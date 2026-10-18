//! PenaDF core: every document operation the desktop app offers, as plain
//! functions over PDF bytes so they can be tested without a UI.
//!
//! Feature scope follows the tools Adobe lists for Acrobat on
//! <https://www.adobe.com/acrobat/complete-pdf-solution.html> and its features pages,
//! limited to what works offline on one machine.

pub mod engine;
pub mod geometry;
pub mod gui;
pub mod instance;
pub mod ops;
pub mod page_range;
pub mod winshell;
