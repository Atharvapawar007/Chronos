//! Chronos terminal UI: a card-based, theme-aware interface.
//!
//! - `theme`: palettes (8 colourful + 1 monochrome) and terminal colour-depth adaptation
//! - `ui`: small drawing toolkit (cards, meters, charts, centring helpers)
//! - `ring`: the circular timer widget and big-digit clock
//! - `shell`: frame, tab strip, footer, zen mode
//! - `views`: one module per screen
//! - `modals`: pop-up dialogs

pub mod modals;
pub mod ring;
pub mod shell;
pub mod theme;
pub mod ui;
pub mod views;
