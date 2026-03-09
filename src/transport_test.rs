use super::*;
use crate::BridgeError;
use muninn_frames::{Frame as WireFrame, decode_frame, encode_frame};
use muninn_frames::Status as WireStatus;
use muninn_kernel::{BackpressureConfig, Frame as KernelFrame, StreamController, Subscriber};
use serde_json::{Map, Value};
use tokio::sync::mpsc;

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
        data: object(serde_json::json!({
            "name": "note",
            "x": 10,
        })),
    }
}

#[tokio::test]
async fn send_bytes_decodes_and_submits_to_kernel() {
    let (sender, mut rx) = mpsc::channel(1);

    let wire = sample_wire_frame();
    let bytes = encode_frame(&wire);
    let kernel_frame = send_bytes(&bytes, &sender).await.expect("send");

    let received = rx.recv().await.expect("kernel frame");
    assert_eq!(kernel_frame.id, received.id);
    assert_eq!(received.call, "object:create");
}

#[tokio::test]
async fn send_frame_converts_and_submits_to_kernel() {
    let (sender, mut rx) = mpsc::channel(1);

    let kernel_frame = send_frame(sample_wire_frame(), &sender)
        .await
        .expect("send");
    let received = rx.recv().await.expect("kernel frame");

    assert_eq!(received.id, kernel_frame.id);
    assert_eq!(received.call, "object:create");
}

#[tokio::test]
async fn recv_bytes_encodes_subscriber_frame() {
    let request = KernelFrame::request("test:ping");
    let response = request.done();
    let response_id = response.id;
    let (tx, rx) = mpsc::channel(1);
    let controller = StreamController::new(tx.clone(), BackpressureConfig::default());
    let mut subscriber = Subscriber::new(rx, controller);
    tx.send(response).await.expect("send");

    let bytes = recv_bytes(&mut subscriber)
        .await
        .expect("recv")
        .expect("bytes");
    let wire = decode_frame(&bytes).expect("decode");

    assert_eq!(wire.id, response_id.to_string());
    assert_eq!(wire.status, WireStatus::Done);
}

#[tokio::test]
async fn forward_bytes_pushes_to_outbound_channel() {
    let (outbound_tx, mut outbound_rx) = mpsc::channel(1);
    let request = KernelFrame::request("test:ping");
    let response = request.done();
    let response_id = response.id;
    let (tx, rx) = mpsc::channel(1);
    let controller = StreamController::new(tx.clone(), BackpressureConfig::default());
    let mut subscriber = Subscriber::new(rx, controller);
    tx.send(response).await.expect("send");

    let forwarded = forward_bytes(&mut subscriber, &outbound_tx)
        .await
        .expect("forward")
        .expect("frame");
    let bytes = outbound_rx.recv().await.expect("bytes");
    let wire = decode_frame(&bytes).expect("decode");

    assert_eq!(forwarded.id, response_id);
    assert_eq!(wire.id, response_id.to_string());
}

#[tokio::test]
async fn send_bytes_reports_closed_kernel_channel() {
    let (tx, rx) = mpsc::channel(1);
    drop(rx);

    let err = send_bytes(&encode_frame(&sample_wire_frame()), &tx)
        .await
        .expect_err("closed channel");

    assert!(matches!(err, TransportError::KernelChannelClosed));
}

#[tokio::test]
async fn send_frame_preserves_bridge_validation_errors() {
    let (tx, _rx) = mpsc::channel(1);
    let mut frame = sample_wire_frame();
    frame.id = "not-a-uuid".to_owned();

    let err = send_frame(frame, &tx).await.expect_err("invalid frame");

    assert!(matches!(
        err,
        TransportError::Bridge(BridgeError::InvalidUuid {
            field: "id",
            ..
        })
    ));
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(map) = value else {
        panic!("expected JSON object");
    };
    map
}
