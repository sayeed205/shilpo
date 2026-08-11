# Screen capture crate architecture (Screenshot-Only)

Shilpo's screen capture feature is a native implementation replacing the
legacy `grim` shell-out approach. Originally in `desktop/capture`, screen capture has been absorbed into `shilpo-services::capture` as a focused domain module.

## Scope & Deferral of Screen Recording

Screen recording was previously implemented using `ffmpeg-next` and PipeWire audio capture.
Due to upstream FFmpeg ABI instability (such as FFmpeg 7/9 struct opaque migration breaking Rust bindings)
and to keep the shell runtime lightweight and reliable, **screen recording has been completely removed**
and intentionally deferred until the shell architecture stabilizes. No specific future recording backend
(libobs, Avio, GStreamer, or CLI subprocess) is pre-selected.

`shilpo-services::capture` is now a focused, deep module dedicated exclusively to one-shot screenshot capture.

## Placement: `desktop/services::capture` (Linux-only)

Screen capture lives in `shilpo-services` because it depends on Wayland protocols (`wlr-screencopy-unstable-v1`) —
Linux-specific. A future cross-platform abstraction layer can be extracted when macOS/Windows backends
exist (ADR-0001 principle).

## Wayland protocol support

Screen capture uses `wlr-screencopy-unstable-v1`, which is supported by Niri and delivers frames
through SHM memory buffers (using `memfd` for anonymous buffer creation).

**Rejected alternative:** XDG Desktop Portal (`ashpd`) as primary. The portal shows permission dialogs on
every capture — unnecessary friction when Shilpo IS the trusted shell environment.

## One-shot Capture Architecture

The `shilpo-services::capture` API exposes synchronous functions for one-shot screen capture:

```
Wayland Compositor → wlr-screencopy → SHM Buffer → Frame → frame_to_rgba → RgbaImage
                                                                            ├── Clipboard
                                                                            ├── File Save (PNG)
                                                                            └── Future OCR integration
```

- `create_backend()` creates a `Box<dyn CaptureBackend>` providing `capture_frame(output: Option<&str>)`.
- `frame_to_rgba()` converts pixel formats (ARGB8888, XRGB8888, ABGR8888, XBGR8888) into standard RGBA images with buffer truncation checking.
- `crop_image()` handles rectangular region clipping with bounds enforcement.
- `copy_image_to_clipboard()` writes RGBA image data to the system clipboard through the desktop clipboard integration.

## Supported Screenshot Workflow Scope

- **Fullscreen & Output Selection**: Primary or named display output capture.
- **Region Selection**: Region cropping with bounds validation and empty region handling.
- **Intents**: Clipboard copy and PNG save for annotation/menu workflows. OCR is reserved for a future feature and reports an explicit unavailable error today.
