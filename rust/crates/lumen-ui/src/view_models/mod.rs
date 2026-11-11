//! View-model layer: the ported QML bridge (`qml_app/bridge/`).
//!
//! Qt list models become plain structs with accessor methods the egui shell
//! reads each frame; the QObject bridge (`app_bridge`) owns an
//! [`AppController`](lumen_app::app_controller::AppController) handle and
//! mirrors its [`AppEvent`](lumen_app::app_controller::AppEvent) stream into
//! cached UI state.

pub mod app_bridge;
pub mod configs_helpers;
pub mod history_helpers;
pub mod log_model;
pub mod node_edit_helpers;
pub mod node_list_model;
pub mod process_model;
pub mod zapret_helpers;

pub use app_bridge::AppBridge;
