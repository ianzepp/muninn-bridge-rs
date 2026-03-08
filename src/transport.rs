//! Boundary-focused helpers for common gateway byte/frame loops.
//!
//! These helpers remove repetitive decode/convert/send and
//! recv/convert/encode/write boilerplate at transport edges without changing
//! the stream-first kernel model.

use tokio::sync::mpsc;

use crate::{BridgeError, decode_to_kernel, encode_from_kernel, wire_to_kernel};
use muninn_frames::Frame as WireFrame;
use muninn_kernel::{Frame, Subscriber};

/// Errors returned by transport-loop helpers.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// Boundary decoding or conversion failed.
    #[error(transparent)]
    Bridge(#[from] BridgeError),
    /// The kernel ingress channel was closed before the frame could be sent.
    #[error("kernel channel closed")]
    KernelChannelClosed,
    /// The outbound byte channel was closed before the payload could be sent.
    #[error("outbound byte channel closed")]
    OutboundChannelClosed,
}

/// Decode inbound wire bytes, convert them to a kernel frame, and submit the
/// result to the kernel ingress channel.
///
/// Returns the submitted kernel frame so gateway code can inspect or log it
/// without decoding twice.
///
/// # Errors
///
/// Returns [`TransportError`] if decoding/conversion fails or if the kernel
/// ingress channel is closed.
pub async fn send_bytes(
    bytes: &[u8],
    kernel_tx: &mpsc::Sender<Frame>,
) -> Result<Frame, TransportError> {
    let frame = decode_to_kernel(bytes)?;
    kernel_tx
        .send(frame.clone())
        .await
        .map_err(|_| TransportError::KernelChannelClosed)?;
    Ok(frame)
}

/// Convert an inbound wire frame to a kernel frame and submit it to the kernel
/// ingress channel.
///
/// Returns the submitted kernel frame.
///
/// # Errors
///
/// Returns [`TransportError`] if conversion fails or if the kernel ingress
/// channel is closed.
pub async fn send_frame(
    frame: WireFrame,
    kernel_tx: &mpsc::Sender<Frame>,
) -> Result<Frame, TransportError> {
    let kernel_frame = wire_to_kernel(frame)?;
    kernel_tx
        .send(kernel_frame.clone())
        .await
        .map_err(|_| TransportError::KernelChannelClosed)?;
    Ok(kernel_frame)
}

/// Receive the next outbound kernel frame from a subscriber and encode it for
/// transport.
///
/// Returns `Ok(None)` when the subscriber channel is closed.
///
/// # Errors
///
/// This function is infallible with respect to encoding, but keeps a `Result`
/// shape for symmetry with the inbound helpers and future boundary errors.
pub async fn recv_bytes(
    subscriber: &mut Subscriber,
) -> Result<Option<Vec<u8>>, TransportError> {
    let Some(frame) = subscriber.recv().await else {
        return Ok(None);
    };

    Ok(Some(encode_from_kernel(&frame)))
}

/// Receive the next outbound kernel frame from a subscriber, encode it, and
/// forward the bytes to an outbound transport channel.
///
/// Returns the original kernel frame after forwarding so the caller can inspect
/// or log it. Returns `Ok(None)` when the subscriber channel is closed.
///
/// # Errors
///
/// Returns [`TransportError::OutboundChannelClosed`] if the outbound byte
/// channel is closed before the payload can be sent.
pub async fn forward_bytes(
    subscriber: &mut Subscriber,
    outbound_tx: &mpsc::Sender<Vec<u8>>,
) -> Result<Option<Frame>, TransportError> {
    let Some(frame) = subscriber.recv().await else {
        return Ok(None);
    };

    outbound_tx
        .send(encode_from_kernel(&frame))
        .await
        .map_err(|_| TransportError::OutboundChannelClosed)?;

    Ok(Some(frame))
}
