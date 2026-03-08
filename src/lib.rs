//! Strict boundary adapter between `muninn-kernel` and `muninn-frames`.
//!
//! The kernel frame is the canonical in-memory protocol. This crate converts
//! to and from the wire frame only at explicit transport boundaries.

use muninn_frames::{CodecError, Frame as WireFrame, Status as WireStatus, decode_frame, encode_frame};
use muninn_kernel::{Data, Frame as KernelFrame, Status as KernelStatus};
use serde_json::{Map, Value};
use uuid::Uuid;

/// Errors returned while crossing the kernel ↔ wire boundary.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// Protobuf or wire-level decoding failed.
    #[error(transparent)]
    Codec(#[from] CodecError),
    /// A wire ID field was not a valid UUID string.
    #[error("invalid {field} UUID: {value}")]
    InvalidUuid {
        field: &'static str,
        value: String,
    },
    /// The wire payload must be a JSON object when entering the kernel.
    #[error("wire data must be a JSON object, got {kind}")]
    NonObjectData {
        kind: &'static str,
    },
}

/// Convert a wire frame into the canonical in-memory kernel frame.
///
/// # Errors
///
/// Returns [`BridgeError`] if any UUID fields are invalid or if the wire data
/// is not a JSON object.
pub fn wire_to_kernel(frame: WireFrame) -> Result<KernelFrame, BridgeError> {
    Ok(KernelFrame {
        id: parse_uuid("id", frame.id)?,
        parent_id: frame
            .parent_id
            .map(|value| parse_uuid("parent_id", value))
            .transpose()?,
        created_ms: frame.created_ms,
        expires_in: frame.expires_in,
        from: frame.from,
        call: frame.call,
        status: wire_status_to_kernel(frame.status),
        trace: frame.trace,
        data: value_to_data(frame.data)?,
    })
}

/// Convert a kernel frame into the transport wire frame.
#[must_use]
pub fn kernel_to_wire(frame: KernelFrame) -> WireFrame {
    WireFrame {
        id: frame.id.to_string(),
        parent_id: frame.parent_id.map(|value| value.to_string()),
        created_ms: frame.created_ms,
        expires_in: frame.expires_in,
        from: frame.from,
        call: frame.call,
        status: kernel_status_to_wire(frame.status),
        trace: frame.trace,
        data: Value::Object(Map::from_iter(frame.data)),
    }
}

/// Decode protobuf bytes and convert directly into a kernel frame.
///
/// # Errors
///
/// Returns [`BridgeError`] if decoding fails or if the resulting wire frame
/// cannot be converted into a valid kernel frame.
pub fn decode_to_kernel(bytes: &[u8]) -> Result<KernelFrame, BridgeError> {
    let frame = decode_frame(bytes)?;
    wire_to_kernel(frame)
}

/// Convert a kernel frame to a wire frame and encode it as protobuf bytes.
#[must_use]
pub fn encode_from_kernel(frame: &KernelFrame) -> Vec<u8> {
    let wire = kernel_to_wire(frame.clone());
    encode_frame(&wire)
}

fn parse_uuid(field: &'static str, value: String) -> Result<Uuid, BridgeError> {
    Uuid::parse_str(&value).map_err(|_| BridgeError::InvalidUuid { field, value })
}

fn value_to_data(value: Value) -> Result<Data, BridgeError> {
    match value {
        Value::Object(map) => Ok(map.into_iter().collect()),
        other => Err(BridgeError::NonObjectData {
            kind: value_kind(&other),
        }),
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn wire_status_to_kernel(status: WireStatus) -> KernelStatus {
    match status {
        WireStatus::Request => KernelStatus::Request,
        WireStatus::Item => KernelStatus::Item,
        WireStatus::Bulk => KernelStatus::Bulk,
        WireStatus::Done => KernelStatus::Done,
        WireStatus::Error => KernelStatus::Error,
        WireStatus::Cancel => KernelStatus::Cancel,
    }
}

fn kernel_status_to_wire(status: KernelStatus) -> WireStatus {
    match status {
        KernelStatus::Request => WireStatus::Request,
        KernelStatus::Item => WireStatus::Item,
        KernelStatus::Bulk => WireStatus::Bulk,
        KernelStatus::Done => WireStatus::Done,
        KernelStatus::Error => WireStatus::Error,
        KernelStatus::Cancel => WireStatus::Cancel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_wire_frame() -> WireFrame {
        WireFrame {
            id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
            parent_id: Some("550e8400-e29b-41d4-a716-446655440001".to_owned()),
            created_ms: 100,
            expires_in: 25,
            from: Some("user-1".to_owned()),
            call: "object:create".to_owned(),
            status: WireStatus::Request,
            trace: Some(serde_json::json!({ "room": "alpha" })),
            data: serde_json::json!({
                "name": "note",
                "x": 10,
            }),
        }
    }

    #[test]
    fn wire_to_kernel_preserves_fields() {
        let kernel = wire_to_kernel(sample_wire_frame()).expect("convert");

        assert_eq!(
            kernel.id,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("uuid")
        );
        assert_eq!(kernel.parent_id.is_some(), true);
        assert_eq!(kernel.call, "object:create");
        assert_eq!(kernel.status, KernelStatus::Request);
        assert_eq!(kernel.trace, Some(serde_json::json!({ "room": "alpha" })));
        assert_eq!(kernel.data.get("name"), Some(&serde_json::json!("note")));
    }

    #[test]
    fn wire_to_kernel_rejects_invalid_id() {
        let mut frame = sample_wire_frame();
        frame.id = "not-a-uuid".to_owned();

        let err = wire_to_kernel(frame).expect_err("invalid uuid");
        assert!(matches!(
            err,
            BridgeError::InvalidUuid {
                field: "id",
                ..
            }
        ));
    }

    #[test]
    fn wire_to_kernel_rejects_non_object_data() {
        let mut frame = sample_wire_frame();
        frame.data = serde_json::json!(["not", "an", "object"]);

        let err = wire_to_kernel(frame).expect_err("non-object data");
        assert!(matches!(err, BridgeError::NonObjectData { kind: "array" }));
    }

    #[test]
    fn kernel_to_wire_preserves_fields() {
        let kernel = KernelFrame {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("uuid"),
            parent_id: Some(
                Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").expect("uuid"),
            ),
            created_ms: 100,
            expires_in: 25,
            from: Some("user-1".to_owned()),
            call: "object:create".to_owned(),
            status: KernelStatus::Done,
            trace: Some(serde_json::json!({ "room": "alpha" })),
            data: Data::from([
                ("name".to_owned(), serde_json::json!("note")),
                ("x".to_owned(), serde_json::json!(10)),
            ]),
        };

        let wire = kernel_to_wire(kernel);
        assert_eq!(wire.id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(wire.status, WireStatus::Done);
        assert_eq!(wire.data, serde_json::json!({ "name": "note", "x": 10 }));
    }

    #[test]
    fn decode_to_kernel_decodes_and_converts() {
        let wire = sample_wire_frame();
        let bytes = encode_frame(&wire);

        let kernel = decode_to_kernel(&bytes).expect("decode");
        assert_eq!(kernel.call, "object:create");
        assert_eq!(kernel.status, KernelStatus::Request);
    }

    #[test]
    fn encode_from_kernel_encodes_wire_bytes() {
        let kernel = KernelFrame::request("test:ping");
        let bytes = encode_from_kernel(&kernel);
        let wire = decode_frame(&bytes).expect("decode");

        assert_eq!(wire.id, kernel.id.to_string());
        assert_eq!(wire.call, "test:ping");
        assert_eq!(wire.status, WireStatus::Request);
    }
}
