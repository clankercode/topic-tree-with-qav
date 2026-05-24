//! Wire protocol types — single source of truth.
//!
//! Phase 0 scaffold: only the envelope + `Hello`/`Welcome` skeleton so that
//! `just proto-gen` has something to export. Real message variants land in
//! Phase 1+ alongside the routes that consume them.
//!
//! Convention (see `protocol.md`): JSON on the wire is camelCase, Rust is
//! snake_case via `serde(rename_all = "camelCase")`.

use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-gen")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Host,
    Guest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub v: u8,
    pub role: Role,
    pub guest_id: String,
    pub display_name: Option<String>,
    pub admin_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct You {
    pub client_id: String,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct Welcome {
    pub v: u8,
    pub ts: u64,
    pub seq: u64,
    pub you: You,
}

#[cfg(all(test, feature = "ts-gen"))]
mod proto_export_tests {
    use super::*;

    // Triggered by `just proto-gen`:
    //   cargo test --features ts-gen proto_export -- --nocapture
    // ts-rs writes each `#[ts(export)]` type to the configured path when its
    // `export()` is invoked; calling it explicitly is a robustness measure.
    #[test]
    fn proto_export() {
        Role::export().expect("export Role");
        Hello::export().expect("export Hello");
        You::export().expect("export You");
        Welcome::export().expect("export Welcome");
    }
}
