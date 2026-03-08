# muninn-bridge

Strict boundary adapter between the in-memory kernel protocol and the wire transport protocol.

`muninn-bridge` is the boundary half of the Muninn core stack:

- **`muninn-kernel`** — canonical in-memory frame model and async routing primitives
- **`muninn-frames`** — wire `Frame` type plus protobuf encoding and decoding
- **`muninn-bridge`** — strict conversion between the two at explicit I/O boundaries

This crate is intentionally narrow. It does not route frames, register handlers, or define transport sessions. Its job is to keep kernel frames in memory until a real transport boundary is reached, then convert cleanly and predictably.

## Installation

Add to your `Cargo.toml` as a git dependency:

```toml
[dependencies]
bridge = { package = "muninn-bridge", git = "https://github.com/ianzepp/muninn-bridge.git" }
```

Or with the full package name in `use` statements:

```toml
[dependencies]
muninn-bridge = { git = "https://github.com/ianzepp/muninn-bridge.git" }
```

Pin to a specific tag or commit:

```toml
[dependencies]
bridge = { package = "muninn-bridge", git = "https://github.com/ianzepp/muninn-bridge.git", tag = "v0.1.0" }
```

## Public API

The entire public surface is four functions and one error type:

```rust
pub fn wire_to_kernel(frame: muninn_frames::Frame)
    -> Result<muninn_kernel::Frame, BridgeError>;
pub fn kernel_to_wire(frame: muninn_kernel::Frame) -> muninn_frames::Frame;

pub fn decode_to_kernel(bytes: &[u8])
    -> Result<muninn_kernel::Frame, BridgeError>;
pub fn encode_from_kernel(frame: &muninn_kernel::Frame) -> Vec<u8>;

pub enum BridgeError {
    Codec(muninn_frames::CodecError),
    InvalidUuid { field: &'static str, value: String },
    NonObjectData { kind: &'static str },
}
```

## Design

Muninn is stream-first:

- the native operation shape is one request followed by zero or more response frames
- terminal status is explicit (`Done`, `Error`, or `Cancel`)
- sync and collect/block adapters are derived above the kernel
- serialization is deferred until a mandatory network or storage boundary

That makes `muninn-kernel::Frame` the canonical in-memory protocol. `muninn-frames::Frame` is a boundary type, not the default application model.

## Conversion Rules

The bridge is strict by default:

- `id` and `parent_id` must parse as UUIDs
- wire `data` must be a JSON object when entering the kernel
- `status` maps one-to-one between crates
- `trace` is passed through unchanged
- kernel `data` becomes a JSON object when crossing to the wire

Invalid identifiers are errors. The bridge never fabricates replacement UUIDs, because that would break request/response correlation.

## Usage

### Wire Bytes to Kernel Frame

```rust
use bridge::decode_to_kernel;

let kernel_frame = decode_to_kernel(&incoming_bytes)?;
```

### Kernel Frame to Wire Bytes

```rust
use bridge::encode_from_kernel;
use muninn_kernel::Frame;

let request = Frame::request("object:list");
let bytes = encode_from_kernel(&request);
```

### Explicit Frame Conversion

```rust
use bridge::{kernel_to_wire, wire_to_kernel};

let wire = muninn_frames::Frame {
    id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
    parent_id: None,
    created_ms: 1709913600000,
    expires_in: 0,
    from: Some("user-42".to_owned()),
    call: "object:create".to_owned(),
    status: muninn_frames::Status::Request,
    trace: None,
    data: serde_json::json!({ "name": "hello" }),
};

let kernel = wire_to_kernel(wire)?;
let round_trip = kernel_to_wire(kernel);
```

## Relationship to muninn-kernel and muninn-frames

Use `muninn-kernel` for in-process messaging and routing. Use `muninn-frames` when you must cross a transport boundary. Use `muninn-bridge` exactly at the seam between them.

Typical boundary flow:

```text
socket bytes
  -> muninn-frames::decode_frame()
  -> muninn-bridge::wire_to_kernel()
  -> muninn-kernel routing
  -> muninn-bridge::kernel_to_wire()
  -> muninn-frames::encode_frame()
  -> socket bytes
```

## Notes

- **Kernel frames stay in memory as long as possible.** This crate exists to delay serialization until it is required.
- **Non-object wire payloads are rejected on kernel entry.** The kernel expects flat object-like data.
- **Decoding can fail at two layers.** First as protobuf (`CodecError`), then as bridge validation (`BridgeError`).
- **No transport policy is imposed here.** WebSocket, TCP, HTTP, and file boundaries should wrap these functions rather than being built into them.

## Status

The API is intentionally small and early-stage. Pin to a tag or revision rather than tracking a moving branch.
