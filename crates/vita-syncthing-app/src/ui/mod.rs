//! Presentation & Rendering Layer (`ui/`).
//! Pure view layer: renders `&AppState` onto `Gfx` and presents via `VitaFramebuffer`.
//! Contains zero business logic, configuration persistence, or network I/O.

pub mod gfx;
pub mod image;
pub mod modals;
pub mod renderer;
pub mod screens;
pub mod theme;
pub mod widgets;

pub use renderer::VitaUiRenderer;
