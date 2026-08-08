use std::path::PathBuf;
use std::time::Duration;

use shilpo_capture::backend::CaptureBackend;
use shilpo_capture::backend::test::TestBackend;
use shilpo_capture::{
    AudioSource, RecordingCommand, RecordingController, RecordingRequest, RecordingSource,
    RecordingState, StreamConfig, capture_frame, create_backend, enumerate_sources,
};

#[test]
#[ignore = "requires a live Wayland compositor"]
fn production_backend_never_falls_back_to_synthetic_frames() {
    let mut backend = create_backend().expect("native backend should be available");
    let frame = backend
        .capture_frame(None)
        .expect("capture should return pixels");
    assert!(frame.width > 0 && frame.height > 0);
}

#[test]
#[ignore = "requires a live Wayland compositor"]
fn test_capture_frame_api() {
    let frame = capture_frame(None).expect("capture API should return pixels");
    assert!(frame.width > 0 && frame.height > 0);
}

#[test]
#[ignore = "requires a live Wayland compositor"]
fn test_enumerate_sources() {
    let sources = enumerate_sources().expect("source enumeration should work");
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
#[ignore = "requires a live Wayland compositor"]
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
        .expect("recording should start");
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

#[test]
fn test_media_pipeline_integration_mp4() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let output_dir = temp_dir.path().to_path_buf();

    let config = StreamConfig {
        framerate: 30,
        quality: shilpo_capture::Quality::Balanced,
        output_dir,
    };

    let first_frame = TestBackend::generate_solid_frame(640, 480, 255, 0, 0);
    let _transformed = shilpo_capture::pipeline::transform::transform_frame(&first_frame)
        .expect("transform frame");
    let mut encoder = shilpo_capture::pipeline::video_encode::VideoEncoder::new(640, 480, &config)
        .expect("create video encoder");
    let mut audio_encoder =
        shilpo_capture::pipeline::audio_encode::AudioEncoder::new().expect("create audio encoder");
    let mut muxer =
        shilpo_capture::pipeline::mux::Muxer::new(&config, &encoder, Some(&audio_encoder))
            .expect("create muxer");

    for i in 0..10 {
        let frame = TestBackend::generate_solid_frame(640, 480, i * 20, 100, 150);
        let transformed =
            shilpo_capture::pipeline::transform::transform_frame(&frame).expect("transform frame");
        let packets = encoder.encode_frame(&transformed).expect("encode frame");
        for pkt in packets {
            muxer.write_video_packet(&pkt).expect("write video packet");
        }

        let audio_buf = shilpo_capture::pipeline::audio_capture::AudioBuffer {
            pcm_data: vec![0.1f32; 960 * 2],
            sample_rate: 48000,
            channels: 2,
        };
        let audio_pkts = audio_encoder
            .encode_buffer(&audio_buf)
            .expect("encode audio");
        for apkt in audio_pkts {
            muxer.write_audio_packet(&apkt).expect("write audio packet");
        }
    }

    for pkt in encoder.flush().expect("flush video") {
        muxer
            .write_video_packet(&pkt)
            .expect("write flushed packet");
    }
    for pkt in audio_encoder.flush().expect("flush audio") {
        muxer
            .write_audio_packet(&pkt)
            .expect("write flushed audio packet");
    }

    let (final_path, _duration) = muxer
        .finalize(Duration::from_secs(1))
        .expect("finalize muxer");
    assert!(final_path.exists());
    assert_eq!(final_path.extension().unwrap(), "mp4");

    ffmpeg::init().unwrap();
    let ictx = ffmpeg::format::input(&final_path).expect("open input mp4 file");
    let video_stream = ictx
        .streams()
        .best(ffmpeg::media::Type::Video)
        .expect("video stream exists");
    assert_eq!(video_stream.parameters().id(), ffmpeg::codec::Id::H264);

    let audio_stream = ictx
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .expect("audio stream exists");
    assert_eq!(audio_stream.parameters().id(), ffmpeg::codec::Id::AAC);
}
