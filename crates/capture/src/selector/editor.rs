use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationTool {
    Pen,
    Highlighter,
    Rectangle,
    Arrow,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnnotationElement {
    Freehand {
        points: Vec<(f32, f32)>,
        color: [u8; 4],
        stroke_width: f32,
        is_highlighter: bool,
    },
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [u8; 4],
        stroke_width: f32,
    },
    Arrow {
        start: (f32, f32),
        end: (f32, f32),
        color: [u8; 4],
        stroke_width: f32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        color: [u8; 4],
        size: f32,
    },
}

#[derive(Debug, Clone)]
pub struct AnnotationEditorState {
    base_image: RgbaImage,
    elements: Vec<AnnotationElement>,
    undo_stack: Vec<Vec<AnnotationElement>>,
    redo_stack: Vec<Vec<AnnotationElement>>,
    active_tool: AnnotationTool,
    stroke_width: f32,
}

impl AnnotationEditorState {
    pub fn new(base_image: RgbaImage) -> Self {
        Self {
            base_image,
            elements: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            active_tool: AnnotationTool::Pen,
            stroke_width: 4.0,
        }
    }

    pub fn set_tool(&mut self, tool: AnnotationTool) {
        self.active_tool = tool;
    }

    pub fn set_stroke_width(&mut self, width: f32) {
        self.stroke_width = width.max(1.0);
    }

    pub fn add_element(&mut self, element: AnnotationElement) {
        self.undo_stack.push(self.elements.clone());
        self.redo_stack.clear();
        self.elements.push(element);
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.elements.clone());
            self.elements = prev;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.elements.clone());
            self.elements = next;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        if !self.elements.is_empty() {
            self.undo_stack.push(self.elements.clone());
            self.redo_stack.clear();
            self.elements.clear();
        }
    }

    /// Render annotations onto the base image to produce the exported PNG buffer.
    pub fn render_final_image(&self) -> RgbaImage {
        let mut canvas = self.base_image.clone();
        let (width, height) = canvas.dimensions();

        for elem in &self.elements {
            match elem {
                AnnotationElement::Freehand {
                    points,
                    color,
                    stroke_width,
                    is_highlighter,
                } => {
                    let radius = (*stroke_width / 2.0).max(1.0) as i32;
                    let alpha = if *is_highlighter {
                        (color[3] as f32 * 0.4) as u8
                    } else {
                        color[3]
                    };
                    for &(px, py) in points {
                        let cx = px as i32;
                        let cy = py as i32;
                        for dx in -radius..=radius {
                            for dy in -radius..=radius {
                                let x = cx + dx;
                                let y = cy + dy;
                                if x >= 0
                                    && y >= 0
                                    && (x as u32) < width
                                    && (y as u32) < height
                                    && dx * dx + dy * dy <= radius * radius
                                {
                                    let existing = canvas.get_pixel(x as u32, y as u32);
                                    let blended = blend_pixel(
                                        *existing,
                                        Rgba([color[0], color[1], color[2], alpha]),
                                    );
                                    canvas.put_pixel(x as u32, y as u32, blended);
                                }
                            }
                        }
                    }
                }
                AnnotationElement::Rect {
                    x,
                    y,
                    width: w,
                    height: h,
                    color,
                    stroke_width,
                } => {
                    let stroke = *stroke_width as u32;
                    let x0 = (*x as u32).min(width);
                    let y0 = (*y as u32).min(height);
                    let x1 = ((*x + *w) as u32).min(width);
                    let y1 = ((*y + *h) as u32).min(height);
                    let px = Rgba(*color);

                    for ix in x0..x1 {
                        for iy in y0..y0 + stroke {
                            if iy < height {
                                canvas.put_pixel(ix, iy, px);
                            }
                        }
                        for iy in y1.saturating_sub(stroke)..y1 {
                            if iy < height {
                                canvas.put_pixel(ix, iy, px);
                            }
                        }
                    }
                    for iy in y0..y1 {
                        for ix in x0..x0 + stroke {
                            if ix < width {
                                canvas.put_pixel(ix, iy, px);
                            }
                        }
                        for ix in x1.saturating_sub(stroke)..x1 {
                            if ix < width {
                                canvas.put_pixel(ix, iy, px);
                            }
                        }
                    }
                }
                AnnotationElement::Arrow {
                    start,
                    end,
                    color,
                    stroke_width,
                } => {
                    // Simple line drawing from start to end
                    let (x0, y0) = (start.0 as i32, start.1 as i32);
                    let (x1, y1) = (end.0 as i32, end.1 as i32);
                    let px = Rgba(*color);
                    draw_line(&mut canvas, x0, y0, x1, y1, px, *stroke_width as i32);
                }
                AnnotationElement::Text {
                    x, y, text, color, ..
                } => {
                    // Render simple text indicator
                    let px = Rgba(*color);
                    let x0 = (*x as i32).max(0) as u32;
                    let y0 = (*y as i32).max(0) as u32;
                    for (i, _ch) in text.chars().enumerate() {
                        let cx = x0 + (i as u32 * 8);
                        if cx < width && y0 < height {
                            for dy in 0..10 {
                                if y0 + dy < height {
                                    canvas.put_pixel(cx, y0 + dy, px);
                                }
                            }
                        }
                    }
                }
            }
        }

        canvas
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let final_image = self.render_final_image();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        final_image
            .save(path)
            .map_err(|e| format!("failed to save annotated image: {e}"))
    }
}

fn blend_pixel(bg: Rgba<u8>, fg: Rgba<u8>) -> Rgba<u8> {
    let a = fg[3] as f32 / 255.0;
    let r = (fg[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8;
    let g = (fg[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8;
    let b = (fg[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8;
    Rgba([r, g, b, 255])
}

fn draw_line(
    img: &mut RgbaImage,
    mut x0: i32,
    mut y0: i32,
    x1: i32,
    y1: i32,
    color: Rgba<u8>,
    width: i32,
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (img_w, img_h) = (img.width() as i32, img.height() as i32);

    loop {
        for wx in -width / 2..=width / 2 {
            for wy in -width / 2..=width / 2 {
                let px = x0 + wx;
                let py = y0 + wy;
                if px >= 0 && py >= 0 && px < img_w && py < img_h {
                    img.put_pixel(px as u32, py as u32, color);
                }
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotation_editor_undo_redo() {
        let base = RgbaImage::new(100, 100);
        let mut editor = AnnotationEditorState::new(base);
        assert!(!editor.undo());

        editor.add_element(AnnotationElement::Rect {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
            color: [255, 0, 0, 255],
            stroke_width: 2.0,
        });

        assert_eq!(editor.elements.len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.elements.len(), 0);
        assert!(editor.redo());
        assert_eq!(editor.elements.len(), 1);
    }
}
