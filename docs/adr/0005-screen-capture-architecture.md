# Screen capture crate architecture

Shilpo's screen capture and recording feature (`shilpo-capture`) is a native implementation replacing the
legacy `grim`/`wf-recorder` shell-out approach. This ADR records the key trade-offs.

## Crate placement: `desktop/capture` (Linux-only)

The crate lives in `desktop/` because it depends on Wayland protocols, PipeWire, and FFmpeg — all
Linux-specific. A future cross-platform abstraction layer can be extracted when macOS/Windows backends
exist, but that's a hypothetical seam with one adapter today (ADR-0001 principle).

## Wayland protocol support

The first release uses `wlr-screencopy-unstable-v1`, which is the protocol currently supported by Niri
and delivers frames through SHM buffers. `ext-image-copy-capture-v1` is intentionally deferred until the
compositor support and DMA-BUF implementation are available; it is not part of the active runtime factory.

**Rejected alternative:** XDG Desktop Portal (`ashpd`) as primary. The portal shows permission dialogs on
every capture — unnecessary friction when Shilpo IS the trusted shell environment. Portal may be added as a
third fallback for non-Niri compositors in the future.

## PipeWire over PulseAudio

Audio capture uses `pipewire-rs` natively instead of `libpulse-binding`. PipeWire is the standard audio
framework on modern Linux desktops (Fedora, Ubuntu 22.04+, Arch). The WIP's PulseAudio code worked through
PipeWire's compatibility layer, but native PipeWire offers better latency, proper screen audio capture, and
aligns with the project's direction (services migrating to native APIs).

## FFmpeg over GStreamer and libobs

Encoding uses `ffmpeg-next` (Rust FFmpeg bindings).

**Rejected: GStreamer** — adds 50-100MB of dependencies for pipeline abstraction that's unnecessary when we
have an explicit fork-join pipeline with `crossbeam-channel`. FFmpeg is already 20-30MB and commonly
pre-installed.

**Rejected: libobs-rs** — OBS Studio bindings require the full OBS library chain (50-100MB+). OBS is
designed as a standalone app with its own threading model and plugin architecture. Its Wayland capture goes
through PipeWire portal anyway, so no advantage over direct screencopy. The dependency weight is
unjustifiable for a desktop shell component.

## Fork-join pipeline with `std::thread`

The recording pipeline uses `std::thread` + `crossbeam-channel` with no async runtime dependency:

```
Video: CaptureBackend → TransformStage → VideoEncodeStage ─┐
                                                             ├→ MuxStage → File
Audio: PipeWireCapture → AudioEncodeStage ──────────────────┘
```

Each stage is a deep module with a small channel-based interface. The capture crate exposes a synchronous
API (`RecordingController::start()`, `stop()`). The shell bridges to GPUI's async model at the integration
layer.

**Rejected: tokio async** — FFmpeg encoding and PipeWire capture are inherently blocking. Wrapping them in
async adds complexity without benefit. Keeping the crate async-runtime-agnostic means it can be used from
tokio, gpui, or bare std.

## Rewrite over move

The WIP prototype at `crates/capture/` is not moved to `desktop/capture/` — it's rewritten using the WIP
as reference. The WIP has a 1442-line god module (`worker.rs`), deprecated protocol code, PulseAudio
dependency, hard-coded OCR dependencies, and mixed async patterns. A fresh implementation following the
settled architecture is cleaner than move-then-refactor.

## First Release Scope & Follow-up Items

The initial production release delivers a truthful, rock-solid baseline:
- **Screenshots**: Full output capture and bounded region selection.
- **Recording**: Full display output recording only (H.264 video + AAC audio in MP4 container).
- **Audio**: System audio or no audio.
- **Protocol**: `wlr-screencopy-unstable-v1` active adapter with persistent session connection, SHM buffer row compaction, and non-blocking frame backpressure handling.

### Deferred Follow-up Work
The following features are intentionally deferred to follow-up issues:
1. Window and custom region video recording.
2. `ext-image-copy-capture-v1` DMA-BUF zero-copy backend protocol.
3. VA-API hardware acceleration for video encoding.
4. Additional video codecs (H.265, VP9, AV1) and container formats (MKV, WebM).
5. Microphone audio capture and mixed system + microphone audio streams.
