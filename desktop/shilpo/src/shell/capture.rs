use std::io::Cursor;
use std::sync::Arc;

use gpui::{
    App, AppContext, Context, FocusHandle, Focusable, Image, ImageFormat, ImageSource,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ObjectFit, ParentElement, Render, Styled, StyledImage, Window, div, img, px,
};
use image::RgbaImage;
use shilpo_m3e::ActiveTheme;
use shilpo_services::capture::{CaptureIntent, Region, copy_image_to_clipboard, crop_image};

use crate::config::CaptureConfig;

/// GPUI-owned selection surface. The compositor frame is frozen before this
/// window is opened, so dragging never changes the pixels underneath it.
pub struct CaptureOverlayView {
    frame: Arc<RgbaImage>,
    frame_source: ImageSource,
    config: CaptureConfig,
    intent: CaptureIntent,
    drag_start: Option<gpui::Point<gpui::Pixels>>,
    selection: Option<Region>,
    focus_handle: FocusHandle,
}

impl CaptureOverlayView {
    pub fn new(
        frame: RgbaImage,
        intent: CaptureIntent,
        config: CaptureConfig,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        let frame = Arc::new(frame);
        let bytes = encode_png(&frame).unwrap_or_default();
        let image = Arc::new(Image::from_bytes(ImageFormat::Png, bytes));
        let focus_handle = _cx.focus_handle();
        window.focus(&focus_handle, _cx);
        Self {
            frame,
            frame_source: ImageSource::Image(image),
            config,
            intent,
            drag_start: None,
            selection: None,
            focus_handle,
        }
    }

    pub fn view(
        frame: RgbaImage,
        intent: CaptureIntent,
        config: CaptureConfig,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Entity<shilpo_m3e::Root> {
        let view = cx.new(|cx| Self::new(frame, intent, config, window, cx));
        cx.new(|cx| {
            shilpo_m3e::Root::new(view, window, cx)
                .bordered(false)
                .bg(gpui::transparent_black())
        })
    }

    fn begin_drag(&mut self, event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.drag_start = Some(event.position);
        self.selection = Some(Region {
            x: event.position.x.as_f32() as i32,
            y: event.position.y.as_f32() as i32,
            width: 0,
            height: 0,
        });
        cx.notify();
    }

    fn update_drag(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(start) = self.drag_start else { return };
        let end = event.position;
        self.selection = Some(region_between(start, end));
        cx.notify();
    }

    fn finish_drag(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(start) = self.drag_start.take() else {
            return;
        };
        let region = region_between(start, event.position);
        if region.is_empty() {
            self.selection = None;
            cx.notify();
            return;
        }

        let image = crop_image(&self.frame, region);
        let result = match self.intent {
            CaptureIntent::Clipboard => copy_image_to_clipboard(&image).map_err(|e| e.to_string()),
            CaptureIntent::Annotation => {
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H-%M-%S");
                let path = self
                    .config
                    .resolved_screenshot_dir()
                    .join(format!("Screenshot from {timestamp}.png"));
                image.save(path).map_err(|error| error.to_string())
            }
            CaptureIntent::Ocr => Err("OCR capture is not available yet".into()),
            CaptureIntent::Menu => Err("capture menu is not implemented".into()),
        };
        if let Err(error) = result {
            tracing::warn!(%error, "capture selection failed");
        }
        window.remove_window();
    }

    fn cancel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }
}

impl Render for CaptureOverlayView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selection = self.selection;
        let frame = div()
            .absolute()
            .inset_0()
            // The captured frame is a complete output image. GPUI defaults image
            // elements to `Contain`, which leaves the layer-shell surface (and
            // the live bar underneath it) visible around the frame. Fill the
            // surface explicitly so the frozen frame owns every output pixel.
            .child(
                img(self.frame_source.clone())
                    .size_full()
                    .object_fit(ObjectFit::Fill),
            );
        let shade = div()
            .absolute()
            .inset_0()
            .bg(cx.theme().scrim.opacity(0.38));
        let selection_box = selection.map(|region| {
            div()
                .absolute()
                .left(px(region.x as f32))
                .top(px(region.y as f32))
                .w(px(region.width as f32))
                .h(px(region.height as f32))
                .border_2()
                .border_color(cx.theme().primary)
                .bg(cx.theme().primary.opacity(0.08))
        });
        div()
            .size_full()
            .cursor_crosshair()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_drag))
            .on_mouse_move(cx.listener(Self::update_drag))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::finish_drag))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::finish_drag))
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.eq_ignore_ascii_case("escape") {
                    view.cancel(window, cx);
                }
            }))
            .track_focus(&self.focus_handle)
            .child(frame)
            .child(shade)
            .children(selection_box)
    }
}

impl Focusable for CaptureOverlayView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn region_between(start: gpui::Point<gpui::Pixels>, end: gpui::Point<gpui::Pixels>) -> Region {
    let sx = start.x.as_f32() as i32;
    let sy = start.y.as_f32() as i32;
    let ex = end.x.as_f32() as i32;
    let ey = end.y.as_f32() as i32;
    Region {
        x: sx.min(ex).max(0),
        y: sy.min(ey).max(0),
        width: (sx - ex).unsigned_abs(),
        height: (sy - ey).unsigned_abs(),
    }
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(bytes.into_inner())
}
