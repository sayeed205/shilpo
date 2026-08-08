use shilpo_capture::backend::CaptureBackend;
use shilpo_capture::backend::test::TestBackend;
use shilpo_capture::{
    Frame, FrameFormat, Rect, Region, capture_frame, copy_image_to_clipboard, create_backend,
    crop_image, frame_to_rgba,
};
use std::time::Instant;

#[test]
#[ignore = "requires a live Wayland compositor"]
fn production_backend_one_shot_capture() {
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
fn test_test_backend_capture() {
    let mut backend = TestBackend::new();
    let frame = backend
        .capture_frame(None)
        .expect("TestBackend capture_frame");
    assert_eq!(frame.width, 1920);
    assert_eq!(frame.height, 1080);
}

#[test]
fn test_pixel_format_conversions() {
    // ARGB / XRGB conversion: [B, G, R, A] -> [R, G, B, A]
    let argb_frame = Frame {
        data: vec![10, 20, 30, 255],
        width: 1,
        height: 1,
        format: FrameFormat::Argb8888,
        timestamp: Instant::now(),
    };
    let rgba = frame_to_rgba(&argb_frame).expect("convert ARGB to RGBA");
    assert_eq!(rgba.get_pixel(0, 0).0, [30, 20, 10, 255]);

    let xrgb_frame = Frame {
        data: vec![10, 20, 30, 0],
        width: 1,
        height: 1,
        format: FrameFormat::Xrgb8888,
        timestamp: Instant::now(),
    };
    let rgba = frame_to_rgba(&xrgb_frame).expect("convert XRGB to RGBA");
    assert_eq!(rgba.get_pixel(0, 0).0, [30, 20, 10, 255]);

    // ABGR / XBGR conversion: [R, G, B, A] -> [R, G, B, A]
    let abgr_frame = Frame {
        data: vec![30, 20, 10, 200],
        width: 1,
        height: 1,
        format: FrameFormat::Abgr8888,
        timestamp: Instant::now(),
    };
    let rgba = frame_to_rgba(&abgr_frame).expect("convert ABGR to RGBA");
    assert_eq!(rgba.get_pixel(0, 0).0, [30, 20, 10, 200]);

    let xbgr_frame = Frame {
        data: vec![30, 20, 10, 0],
        width: 1,
        height: 1,
        format: FrameFormat::Xbgr8888,
        timestamp: Instant::now(),
    };
    let rgba = frame_to_rgba(&xbgr_frame).expect("convert XBGR to RGBA");
    assert_eq!(rgba.get_pixel(0, 0).0, [30, 20, 10, 255]);
}

#[test]
fn test_truncated_frame_buffer_error() {
    let truncated_frame = Frame {
        data: vec![0; 10], // Expected 10 * 10 * 4 = 400 bytes
        width: 10,
        height: 10,
        format: FrameFormat::Argb8888,
        timestamp: Instant::now(),
    };
    let result = frame_to_rgba(&truncated_frame);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("truncated"));
}

#[test]
fn test_region_cropping_and_bounds() {
    let mut img = image::RgbaImage::new(100, 100);
    for pixel in img.pixels_mut() {
        pixel.0 = [255, 0, 0, 255];
    }

    // Normal crop
    let region = Region {
        x: 10,
        y: 10,
        width: 20,
        height: 20,
    };
    let cropped = crop_image(&img, region);
    assert_eq!(cropped.width(), 20);
    assert_eq!(cropped.height(), 20);

    // Empty region
    let empty_region = Rect {
        x: 0,
        y: 0,
        width: 0,
        height: 10,
    };
    assert!(empty_region.is_empty());
    let cropped_empty = crop_image(&img, empty_region);
    assert_eq!(cropped_empty.width(), 0);
    assert_eq!(cropped_empty.height(), 0);

    // Out of bounds crop
    let oob_region = Region {
        x: 90,
        y: 90,
        width: 50,
        height: 50,
    };
    let cropped_oob = crop_image(&img, oob_region);
    assert_eq!(cropped_oob.width(), 10);
    assert_eq!(cropped_oob.height(), 10);

    // Completely outside
    let outside_region = Region {
        x: 200,
        y: 200,
        width: 10,
        height: 10,
    };
    let cropped_outside = crop_image(&img, outside_region);
    assert_eq!(cropped_outside.width(), 0);
    assert_eq!(cropped_outside.height(), 0);
}

#[test]
#[ignore = "requires a running Wayland clipboard manager"]
fn test_clipboard_copy() {
    let img = image::RgbaImage::new(10, 10);
    copy_image_to_clipboard(&img).expect("clipboard copy should succeed");
}
