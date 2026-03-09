//! Strict boundary adapter between `muninn-kernel` and `muninn-frames`.
//!
//! The kernel frame is the canonical in-memory protocol. This crate converts
//! to and from the wire frame only at explicit transport boundaries.

pub mod transport;

use muninn_frames::{CodecError, Frame as WireFrame, Status as WireStatus, decode_frame, encode_frame};
use muninn_kernel::{Caller, Frame as KernelFrame, PipeError, Status as KernelStatus};
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
        data: frame.data.into_iter().collect(),
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
        data: frame.data.into_iter().collect(),
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

/// Send a request through a caller and collect the full response stream.
///
/// This is an adapter layered on top of the stream-first kernel model. The
/// canonical primitive remains `Caller::call`, which yields a response stream.
///
/// # Errors
///
/// Returns [`PipeError`] if the request could not be submitted to the caller.
pub async fn collect_call(
    caller: &Caller,
    request: KernelFrame,
) -> Result<Vec<KernelFrame>, PipeError> {
    let stream = caller.call(request).await?;
    Ok(stream.collect().await)
}

fn parse_uuid(field: &'static str, value: String) -> Result<Uuid, BridgeError> {
    Uuid::parse_str(&value).map_err(|_| BridgeError::InvalidUuid { field, value })
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
#[path = "lib_test.rs"]
mod tests;
