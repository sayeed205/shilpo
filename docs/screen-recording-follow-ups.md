# Screen recording follow-ups

The first release of the native screen-recording flow is intentionally limited to
output capture and the current single-worker pipeline. The following items remain
for a later hardening pass:

- Refactor the recording pipeline into the fork/join stages described by ADR-0005,
  with explicit bounded channels and coordinated cancellation/error propagation.
- Make Wayland stream shutdown interruptible; a worker blocked in
  `blocking_dispatch` must not prevent `stop` from returning.
- Emit `RecordingEvent::FrameDropped` from the capture backend instead of only
  logging dropped frames.
- Drain recording completion and error events in the shell/IPC layer so clients
  receive terminal status consistently.
- Pass the negotiated PipeWire sample rate and channel count through to the audio
  encoder (or resample explicitly) instead of assuming 48 kHz stereo.
- Run the full manual Wayland validation matrix after `./setup install`, including
  start/stop toggle, cancel, multi-output selection, and playback/`ffprobe` checks.

Deferred by the current scope: window/region recording, microphone/Both audio,
hardware encoders, and the ext-image-copy backend.
