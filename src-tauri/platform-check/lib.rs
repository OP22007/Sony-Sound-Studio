//! Compile the actual platform transport and model registry without Tauri/WebKit.
//! Protocol constants stand in for the full protocol module; no simulated driver
//! replaces the platform source under examination.
#[path = "../src/oneplus_models.rs"]
pub mod oneplus_models;

pub mod oneplus {
    pub const SERVICE_UUID: &str = "00001107-d102-11e1-9b23-00025b00a5a5";
    pub const CURRENT_SERVICE_UUID: &str = "0000079a-d102-11e1-9b23-00025b00a5a5";
}

#[path = "../src/transport/mod.rs"]
pub mod transport;
