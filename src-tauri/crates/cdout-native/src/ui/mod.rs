//! Screens. Each is a plain function over `&mut AppState` that emits
//! [`crate::state::Msg`]s into an `out` vector instead of acting, so the
//! same code runs in the app and in headless `egui_kittest` tests.

pub mod approval;
pub mod chat;
pub mod chat_input;
pub mod icons;
pub mod main_window;
pub mod model_select;
pub mod onboarding;
pub mod settings;
pub mod sidebar;
pub mod spotlight;
pub mod theme;
pub mod title_bar;
pub mod widgets;
