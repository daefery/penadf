//! Window-independent pieces of the desktop UI (page view, document state, look),
//! kept in the library so they can be driven by tests without opening a window.

pub mod document;
pub mod tabs;
pub mod theme;
pub mod viewer;
