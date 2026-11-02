//! UIKit boundary. All calls that touch views are made on winit's main thread.
#[cfg(target_os = "ios")]
mod apple;
#[cfg(target_os = "ios")]
pub use apple::*;

#[cfg(not(target_os = "ios"))]
pub fn initialize() -> std::io::Result<()> {
    Ok(())
}

#[repr(C)]
#[derive(Default, Debug)]
pub struct Input {
    pub kind: i32,
    pub code: i32,
    pub x: f64,
    pub y: f64,
}

pub mod setup;
