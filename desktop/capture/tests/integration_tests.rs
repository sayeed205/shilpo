use std::path::PathBuf;
use std::time::Duration;

use shilpo_capture::backend::test::TestBackend;
use shilpo_capture::backend::CaptureBackend;
use shilpo_capture::{
    capture_frame, create_backend, enumerate_sources, AudioSource, RecordingCommand,
    RecordingController, RecordingRequest, RecordingSource, RecordingState, StreamConfig,
};

#[test]
fn test_create_backend_fallback() {
    let mut backend = create_backend().expect("create_backend must return valid backend");
    let frame = backend.capture_frame(None).expect("must capture frame");
    assert!(frame.width > 0);
    assert!(frame.height > 0);
}

#[test]
fn test_capture_frame_api() {
    let frame = capture_frame(None).expect("capture_frame API works");
    assert!(frame.width > 0);
    assert!(frame.height > 0);
}

#[test]
fn test_enumerate_sources() {
    let sources = enumerate_sources().expect("sources enumeration works");
    assert!(!sources.is_empty());
}

#[test]
fn test_test_backend_stream() {
    let mut backend = TestBackend::new();
    let config = StreamConfig {
        framerate: 30,
        ..Default::default()
    };
    let rx = backend
        .start_stream(&RecordingSource::primary(), &config)
        .expect("start_stream works");

    let frame = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("receive frame from stream");
    assert_eq!(frame.width, 1280);
    assert_eq!(frame.height, 720);

    backend.stop_stream();
}

#[test]
fn test_recording_controller_lifecycle() {
    let controller = RecordingController::new();
    assert_eq!(controller.state(), RecordingState::Idle);

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let config = StreamConfig {
        framerate: 30,
        output_dir: PathBuf::from(temp_dir.path()),
        ..Default::default()
    };

    let request = RecordingRequest {
        source: RecordingSource::primary(),
        audio: AudioSource::None,
    };

    // Start recording
    controller
        .start(request, config)
        .expect("controller start works");
    assert!(controller.state().is_recording());

    // Pause & Resume
    controller.pause().expect("pause works");
    assert!(matches!(controller.state(), RecordingState::Paused { .. }));

    controller.resume().expect("resume works");
    assert!(controller.state().is_recording());

    // Stop recording
    controller.stop().expect("stop works");
    assert_eq!(controller.state(), RecordingState::Idle);
}

#[test]
fn test_command_validation() {
    let idle_state = RecordingState::Idle;
    let rec_state = RecordingState::Recording {
        elapsed: Duration::from_secs(5),
    };

    let start_cmd = RecordingCommand::Start(RecordingRequest {
        source: RecordingSource::primary(),
        audio: AudioSource::None,
    });

    assert!(start_cmd.validate(&idle_state).is_ok());
    assert!(start_cmd.validate(&rec_state).is_err());
    assert!(RecordingCommand::Stop.validate(&rec_state).is_ok());
    assert!(RecordingCommand::Stop.validate(&idle_state).is_err());
}
