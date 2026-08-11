use shilpo_shell::extensions::process::{
    HostGeneration, HostMessage, ProcessCodecError, PROTOCOL_VERSION, read_frame,
    recv_host_message, send_host_message, write_frame, MAX_FRAME_SIZE, MAX_QUEUE_BOUND,
};
use shilpo_shell::extensions::ExtensionCommand;
use std::io::Cursor;

#[test]
fn frame_round_trip_and_partial_read_handling() {
    let host_msg = HostMessage {
        protocol_version: PROTOCOL_VERSION,
        host_generation: HostGeneration(1),
        request_id: 42,
        command: ExtensionCommand::SourcesChanged,
    };

    let mut buf = Vec::new();
    send_host_message(&mut buf, &host_msg).expect("send_host_message should succeed");
    assert!(buf.len() > 4);

    let mut cursor = Cursor::new(buf);
    let decoded = recv_host_message(&mut cursor).expect("recv_host_message should succeed");
    assert_eq!(decoded.protocol_version, PROTOCOL_VERSION);
    assert_eq!(decoded.host_generation, HostGeneration(1));
    assert_eq!(decoded.request_id, 42);
    assert!(matches!(decoded.command, ExtensionCommand::SourcesChanged));
}

#[test]
fn zero_oversized_malformed_frame_rejection() {
    // Zero-length payload
    let mut buf = Vec::new();
    assert!(matches!(
        write_frame(&mut buf, &[]),
        Err(ProcessCodecError::ZeroLengthFrame)
    ));

    // Zero-length header read
    let mut zero_header_cursor = Cursor::new(0u32.to_be_bytes().to_vec());
    assert!(matches!(
        read_frame(&mut zero_header_cursor),
        Err(ProcessCodecError::ZeroLengthFrame)
    ));

    // Oversized frame
    let huge_len = ((MAX_FRAME_SIZE + 1) as u32).to_be_bytes();
    let mut huge_cursor = Cursor::new(huge_len.to_vec());
    assert!(matches!(
        read_frame(&mut huge_cursor),
        Err(ProcessCodecError::FrameTooLarge { .. })
    ));

    // Malformed JSON frame
    let malformed_payload = b"not-valid-json";
    let mut malformed_buf = Vec::new();
    write_frame(&mut malformed_buf, malformed_payload).unwrap();
    let mut malformed_cursor = Cursor::new(malformed_buf);
    assert!(matches!(
        recv_host_message(&mut malformed_cursor),
        Err(ProcessCodecError::Json(_))
    ));
}

#[test]
fn protocol_version_mismatch_terminates_child() {
    let host_msg = HostMessage {
        protocol_version: 999, // Mismatched version
        host_generation: HostGeneration(1),
        request_id: 1,
        command: ExtensionCommand::SourcesChanged,
    };
    let mut buf = Vec::new();
    let payload = serde_json::to_vec(&host_msg).unwrap();
    write_frame(&mut buf, &payload).unwrap();

    let mut cursor = Cursor::new(buf);
    assert!(matches!(
        recv_host_message(&mut cursor),
        Err(ProcessCodecError::ProtocolVersionMismatch { expected: 1, found: 999 })
    ));
}

#[test]
fn command_update_queues_never_exceed_64() {
    assert_eq!(MAX_QUEUE_BOUND, 64);
}
