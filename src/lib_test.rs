use super::*;
use muninn_frames::Status as WireStatus;
use muninn_kernel::Status as KernelStatus;

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

#[tokio::test]
async fn collect_call_gathers_streamed_responses() {
    let (mut end_a, mut end_b) = muninn_kernel::pipe(16);
    let caller = end_a.caller();

    let request = KernelFrame::request("test:stream");
    let request_id = request.id;

    let collect_task = tokio::spawn(async move { collect_call(&caller, request).await });

    let received = end_b.recv().await.expect("request");
    end_b.sender().send(received.item(Data::new())).await.expect("item");
    end_b.sender().send(received.done()).await.expect("done");

    let frames = collect_task.await.expect("join").expect("collect");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].status, KernelStatus::Item);
    assert_eq!(frames[0].parent_id, Some(request_id));
    assert_eq!(frames[1].status, KernelStatus::Done);
}
