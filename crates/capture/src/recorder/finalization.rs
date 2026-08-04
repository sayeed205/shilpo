use crate::{
    recorder::encode::{EncState, validate_recording},
    types::{RecordingAudio, RecordingState},
};
use std::{path::PathBuf, time::Duration};

pub struct Finalization {
    pub path: PathBuf,
    pub partial_path: PathBuf,
    pub audio: RecordingAudio,
    pub duration: Duration,
    pub keep: bool,
    pub failure: Option<String>,
    pub warning: Option<String>,
}

/// Flush, validate, and atomically publish one recording, or discard every
/// partial artifact. This is the only module that turns encoder output into a
/// terminal recording state.
pub fn finish(mut encoder: Option<EncState>, request: Finalization) -> RecordingState {
    if let Some(reason) = request.failure {
        drop(encoder.take());
        return failed_after_cleanup(reason, &request.partial_path);
    }

    if !request.keep {
        drop(encoder.take());
        return match remove_partial(&request.partial_path) {
            Ok(()) => RecordingState::Cancelled,
            Err(error) => cleanup_failed(error, request.partial_path),
        };
    }

    let Some(mut encoder) = encoder else {
        return match remove_partial(&request.partial_path) {
            Ok(()) => RecordingState::Cancelled,
            Err(error) => cleanup_failed(error, request.partial_path),
        };
    };

    if let Err(reason) = encoder.flush(request.duration) {
        return failed_after_cleanup(reason, &request.partial_path);
    }
    if encoder.encoded_frames() == 0 {
        return failed_after_cleanup(
            "recording finished without an encoded video frame".into(),
            &request.partial_path,
        );
    }
    if request.audio != RecordingAudio::None && encoder.audio_packets() == 0 {
        return failed_after_cleanup(
            "recording finished without an encoded audio packet".into(),
            &request.partial_path,
        );
    }
    if let Err(reason) =
        validate_recording(&request.partial_path, request.audio != RecordingAudio::None)
    {
        return failed_after_cleanup(reason, &request.partial_path);
    }
    if let Err(error) = std::fs::rename(&request.partial_path, &request.path) {
        return failed_after_cleanup(
            format!("could not publish finalized recording: {error}"),
            &request.partial_path,
        );
    }

    RecordingState::Finished {
        path: request.path,
        duration: request.duration,
        warning: request.warning,
    }
}

fn failed(reason: String, partial_path: Option<PathBuf>) -> RecordingState {
    RecordingState::Failed {
        reason,
        partial_path,
    }
}

fn failed_after_cleanup(reason: String, path: &std::path::Path) -> RecordingState {
    match remove_partial(path) {
        Ok(()) => failed(reason, None),
        Err(error) => failed(
            format!("{reason}; could not remove partial recording: {error}"),
            Some(path.to_path_buf()),
        ),
    }
}

fn cleanup_failed(error: std::io::Error, path: PathBuf) -> RecordingState {
    failed(
        format!("could not remove cancelled recording: {error}"),
        Some(path),
    )
}

fn remove_partial(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_session_removes_partial_file() {
        let directory = tempfile::tempdir().unwrap();
        let partial_path = directory.path().join("recording.part.webm");
        std::fs::write(&partial_path, b"partial").unwrap();
        let state = finish(
            None,
            Finalization {
                path: directory.path().join("recording.webm"),
                partial_path: partial_path.clone(),
                audio: RecordingAudio::None,
                duration: Duration::ZERO,
                keep: false,
                failure: None,
                warning: None,
            },
        );
        assert_eq!(state, RecordingState::Cancelled);
        assert!(!partial_path.exists());
    }

    #[test]
    fn cancellation_reports_an_artifact_that_could_not_be_removed() {
        let directory = tempfile::tempdir().unwrap();
        let partial_path = directory.path().join("recording.part.webm");
        std::fs::create_dir(&partial_path).unwrap();
        let state = finish(
            None,
            Finalization {
                path: directory.path().join("recording.webm"),
                partial_path: partial_path.clone(),
                audio: RecordingAudio::None,
                duration: Duration::ZERO,
                keep: false,
                failure: None,
                warning: None,
            },
        );
        assert!(matches!(
            state,
            RecordingState::Failed {
                partial_path: Some(path),
                ..
            } if path == partial_path
        ));
    }
}
