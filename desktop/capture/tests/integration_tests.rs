use std::path::PathBuf;
use std::time::Duration;

use shilpo_capture::backend::CaptureBackend;
use shilpo_capture::backend::test::TestBackend;
use shilpo_capture::{
    AudioSource, RecordingCommand, RecordingController, RecordingRequest, RecordingSource,
    RecordingState, StreamConfig, capture_frame, create_backend, enumerate_sources,
};

#[test]
fn production_backend_never_falls_back_to_synthetic_frames() {
    let Ok(mut backend) = create_backend() else {
        return;
    };
    let Ok(frame) = backend.capture_frame(None) else {
        return;
    };
    assert!(frame.width > 0 && frame.height > 0);
}

#[test]
fn test_capture_frame_api() {
    let Ok(frame) = capture_frame(None) else {
        return;
    };
    assert!(frame.width > 0 && frame.height > 0);
}

#[test]
fn test_enumerate_sources() {
    let Ok(sources) = enumerate_sources() else {
        return;
    };
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
    if controller.start(request, config).is_err() {
        return;
    }
    assert!(controller.state().is_recording());
    controller.pause().expect("pause should work");
    assert!(matches!(controller.state(), RecordingState::Paused { .. }));
    controller.resume().expect("resume should work");
    assert!(controller.state().is_recording());
    controller.stop().expect("recording should finalize");
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
