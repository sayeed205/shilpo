mod animation;
mod ripple;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use amane::{
    Button as PointerButton, Canvas, Center, Circle, Color, Cursor, Parent, Point, Rectangle,
    Service, Shape, Size, Stack, Text, Weight, Widget,
};

use crate::fonts;
use crate::theme::{self, Theme};

const HEIGHT: f32 = 40.0;
const MIN_WIDTH: f32 = 58.0;
const HORIZONTAL_PADDING: f32 = 16.0;
const REST_RADIUS: f32 = HEIGHT / 2.0;
const PRESSED_RADIUS: f32 = 8.0;

// One contact state per stable logical button name; the spring stays here between redraws.
struct Interactions(HashMap<&'static str, Interaction>);

impl Service for Interactions {
    fn new() -> Self {
        Self(HashMap::new())
    }

    // changes only arrive from the widget's own input handlers
    fn listen() {}
}

impl Interactions {
    fn handle(&mut self, name: &'static str, event: Event) {
        self.handle_at(
            name,
            event,
            true,
            Instant::now(),
            crate::motion::speed(),
            crate::motion::reduced(),
        );
    }

    fn handle_at(
        &mut self,
        name: &'static str,
        event: Event,
        enabled: bool,
        now: Instant,
        speed: f32,
        reduced_motion: bool,
    ) {
        if !enabled {
            return;
        }

        self.0
            .entry(name)
            .or_insert_with(|| Interaction::new_at(now))
            .handle_at(event, now, speed, reduced_motion);
    }
}

#[derive(Clone, Copy)]
enum Event {
    Hover(bool),
    Pointer {
        inside: bool,
        point: Point,
        width: f32,
        height: f32,
    },
    Release,
}

#[derive(Default)]
struct Contact {
    hovered: bool,
    pressed: bool,
}

impl Contact {
    fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Hover(inside) => {
                self.hovered = inside;

                if inside {
                    false
                } else {
                    self.set_pressed(false)
                }
            }
            Event::Pointer { inside, .. } => {
                self.hovered = inside;
                self.set_pressed(inside)
            }
            Event::Release => self.set_pressed(false),
        }
    }

    fn set_pressed(&mut self, pressed: bool) -> bool {
        let changed = self.pressed != pressed;
        self.pressed = pressed;
        changed
    }
}

struct Interaction {
    contact: Contact,
    shape: animation::Spring,
    ripples: Mutex<Vec<ripple::Animation>>,
}

impl Interaction {
    fn new_at(now: Instant) -> Self {
        Self {
            contact: Contact::default(),
            shape: animation::Spring::new_at(REST_RADIUS, now),
            ripples: Mutex::new(Vec::new()),
        }
    }

    fn handle_at(&mut self, event: Event, now: Instant, speed: f32, reduced_motion: bool) {
        let was_pressed = self.contact.pressed;

        if let Some(target) = self.transition(event) {
            self.shape.to_at(target, now, speed);
        }

        match (was_pressed, self.contact.pressed, event) {
            (
                false,
                true,
                Event::Pointer {
                    inside: true,
                    point,
                    width,
                    height,
                },
            ) => self.start_ripple(point, width, height, now, speed, reduced_motion),
            (true, false, _) => self.finish_ripples(now, speed),
            _ => {}
        }
    }

    fn transition(&mut self, event: Event) -> Option<f32> {
        if !self.contact.handle(event) {
            return None;
        }

        Some(if self.contact.pressed {
            PRESSED_RADIUS
        } else {
            REST_RADIUS
        })
    }

    fn start_ripple(
        &mut self,
        point: Point,
        width: f32,
        height: f32,
        now: Instant,
        speed: f32,
        reduced_motion: bool,
    ) {
        let ripples = self
            .ripples
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        for ripple in ripples.iter_mut() {
            ripple.finish(now, speed);
        }

        if !reduced_motion {
            ripples.push(ripple::Animation::new_at(point, width, height, now));
        }
    }

    fn finish_ripples(&mut self, now: Instant, speed: f32) {
        let ripples = self
            .ripples
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        for ripple in ripples {
            ripple.finish(now, speed);
        }
    }

    fn ripple_frames(&self) -> Vec<ripple::Frame> {
        self.ripple_frames_at(
            Instant::now(),
            crate::motion::speed(),
            crate::motion::reduced(),
        )
    }

    fn ripple_frames_at(
        &self,
        now: Instant,
        speed: f32,
        reduced_motion: bool,
    ) -> Vec<ripple::Frame> {
        let mut ripples = self
            .ripples
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if reduced_motion {
            ripples.clear();
            return Vec::new();
        }

        let mut frames = Vec::with_capacity(ripples.len());

        ripples.retain(|ripple| {
            let Some(frame) = ripple.frame(now, speed) else {
                return false;
            };

            frames.push(frame);
            true
        });

        if !ripples.is_empty() {
            amane::request_frame();
        }

        frames
    }
}

/// Draw a filled Material 3 Expressive-style button.
///
/// `name` is a stable, globally unique logical identity, not a label or ordering; it identifies this
/// button across redraws. Its contact and spring state live in this module, and entries currently
/// persist for the process lifetime. Disabled buttons are inert. The activation callback runs only
/// after a left-button release over the same button that received the press.
pub fn filled(
    name: &'static str,
    theme: &Theme,
    label: &str,
    enabled: bool,
    on_activate: impl Fn() + 'static,
) -> Rectangle {
    let text_color = if enabled {
        theme.on_accent
    } else {
        theme::with_opacity(theme.secondary_text, 0.38)
    };

    let label = Text::new(label)
        .size(14.0)
        .font(fonts::BODY)
        .weight(Weight::Medium)
        .color(text_color);

    let label_width = match label.width() {
        Size::Fixed(width) => width,
        Size::Parent => 0.0,
    };

    let width = (label_width + HORIZONTAL_PADDING * 2.0).max(MIN_WIDTH);

    let (radius, hovered, ripples) = if enabled {
        let interactions = Interactions::read();

        interactions
            .0
            .get(name)
            .map(|interaction| {
                (
                    interaction.shape.value(),
                    interaction.contact.hovered,
                    interaction.ripple_frames(),
                )
            })
            .unwrap_or((REST_RADIUS, false, Vec::new()))
    } else {
        (REST_RADIUS, false, Vec::new())
    };

    // Material's disabled colors are 10% on-surface and 38% on-surface-variant.
    let fill = if enabled {
        theme.accent
    } else {
        theme::with_opacity(theme.text, 0.10)
    };

    let mut button = Rectangle::new()
        .width(width)
        .height(HEIGHT)
        .radius(radius)
        .fill(fill)
        .align_child(Center, Center);

    if !enabled {
        return button.child(label);
    }

    let ripple_shapes = ripples
        .into_iter()
        .map(|frame| {
            Box::new(
                Circle::new()
                    .center(frame.center_x, frame.center_y)
                    .radius(frame.radius)
                    .fill(theme.on_accent)
                    .opacity(frame.alpha * 0.1),
            ) as Box<dyn Shape>
        })
        .collect();
    let canvas = Canvas::new()
        .width(Parent)
        .height(Parent)
        .shapes(ripple_shapes);
    let content = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .fill(Color::TRANSPARENT)
        .align_child(Center, Center)
        .child(label);
    let layers = Stack::new(vec![Box::new(content), Box::new(canvas)])
        .width(Parent)
        .height(Parent);

    // Amane clips the layered canvas to this rectangle's current, animated corner radius.
    button = button.clip().child(layers);

    if hovered {
        // A hovered filled button rises to Material's one-step hover elevation.
        button = button.shadow(
            amane::Shadow::drop(Color::BLACK)
                .opacity(0.18)
                .blur(2.0)
                .offset(0.0, 1.0),
        );
    }

    button = button
        .cursor(Cursor::Pointer)
        .on_hover(move |inside| update_contact(name, Event::Hover(inside)))
        .on_drag(move |point| {
            let inside = point.x >= 0.0 && point.x <= width && point.y >= 0.0 && point.y <= HEIGHT;

            update_contact(
                name,
                Event::Pointer {
                    inside,
                    point,
                    width,
                    height: HEIGHT,
                },
            );
        })
        .on_click(move |button| {
            update_contact(name, Event::Release);

            if button == PointerButton::Left {
                on_activate();
            }
        });

    button
}

fn update_contact(name: &'static str, event: Event) {
    Interactions::write().handle(name, event);
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use amane::Point;

    use super::{Event, Interaction, Interactions, PRESSED_RADIUS, REST_RADIUS};

    fn pointer(inside: bool, x: f32, y: f32) -> Event {
        Event::Pointer {
            inside,
            point: Point { x, y },
            width: 120.0,
            height: 40.0,
        }
    }

    fn near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }

    #[test]
    fn events_retarget_the_shape_through_press_leave_drag_and_release() {
        let start = Instant::now();
        let mut interaction = Interaction::new_at(start);

        assert_eq!(interaction.shape.target(), REST_RADIUS);

        interaction.handle_at(pointer(true, 6.0, 10.0), start, 1.0, false);
        assert_eq!(interaction.shape.target(), PRESSED_RADIUS);

        let left = start + Duration::from_millis(10);
        interaction.handle_at(Event::Hover(false), left, 1.0, false);
        assert_eq!(interaction.shape.target(), REST_RADIUS);
        assert!(!interaction.contact.pressed);
        assert!(!interaction.contact.hovered);

        let dragged_back = start + Duration::from_millis(20);
        interaction.handle_at(pointer(true, 30.0, 20.0), dragged_back, 1.0, false);
        assert_eq!(interaction.shape.target(), PRESSED_RADIUS);

        let released = start + Duration::from_millis(30);
        interaction.handle_at(Event::Release, released, 1.0, false);
        assert_eq!(interaction.shape.target(), REST_RADIUS);
        assert!(!interaction.contact.pressed);
        assert!(interaction.contact.hovered);

        interaction.handle_at(
            Event::Release,
            released + Duration::from_millis(10),
            1.0,
            false,
        );
        assert_eq!(interaction.shape.target(), REST_RADIUS);
        assert!(!interaction.contact.pressed);
        assert!(interaction.contact.hovered);
    }

    #[test]
    fn bounded_ripple_uses_the_press_origin_and_material_expansion_and_fade() {
        let start = Instant::now();
        let mut interaction = Interaction::new_at(start);
        interaction.handle_at(pointer(true, 6.0, 10.0), start, 1.0, false);

        let initial = interaction.ripple_frames_at(start, 1.0, false);
        assert_eq!(initial.len(), 1);
        near(initial[0].center_x, 6.0);
        near(initial[0].center_y, 10.0);
        near(initial[0].radius, 36.0);
        near(initial[0].alpha, 0.0);

        let fade_in = start + Duration::from_millis(75);
        let growing = interaction.ripple_frames_at(fade_in, 1.0, false);
        assert_eq!(growing.len(), 1);
        near(growing[0].alpha, 1.0);
        near(growing[0].center_x, 24.0);
        near(growing[0].center_y, 13.333333);
        assert!(growing[0].radius > 36.0);
        assert!(growing[0].radius < 120.0_f32.hypot(40.0) / 2.0 + 10.0);

        let expanded = start + Duration::from_millis(225);
        let full_size = interaction.ripple_frames_at(expanded, 1.0, false);
        near(full_size[0].center_x, 60.0);
        near(full_size[0].center_y, 20.0);
        near(full_size[0].radius, 120.0_f32.hypot(40.0) / 2.0 + 10.0);

        let released = start + Duration::from_millis(300);
        interaction.handle_at(Event::Release, released, 1.0, false);
        let halfway_out = interaction.ripple_frames_at(
            released + Duration::from_millis(75),
            1.0,
            false,
        );
        near(halfway_out[0].alpha, 0.5);
        assert!(interaction
            .ripple_frames_at(released + Duration::from_millis(150), 1.0, false)
            .is_empty());
    }

    #[test]
    fn quick_release_finishes_expansion_before_the_ripple_fades() {
        let start = Instant::now();
        let mut interaction = Interaction::new_at(start);
        interaction.handle_at(pointer(true, 10.0, 20.0), start, 1.0, false);

        let released = start + Duration::from_millis(40);
        interaction.handle_at(Event::Release, released, 1.0, false);

        near(
            interaction.ripple_frames_at(start + Duration::from_millis(224), 1.0, false)[0].alpha,
            1.0,
        );
        near(
            interaction.ripple_frames_at(start + Duration::from_millis(300), 1.0, false)[0].alpha,
            0.5,
        );
        assert!(interaction
            .ripple_frames_at(start + Duration::from_millis(375), 1.0, false)
            .is_empty());
    }

    #[test]
    fn leaving_cancels_the_active_ripple_and_a_new_contact_keeps_its_own_origin() {
        let start = Instant::now();
        let mut interaction = Interaction::new_at(start);
        interaction.handle_at(pointer(true, 8.0, 12.0), start, 1.0, false);

        let cancel = start + Duration::from_millis(20);
        interaction.handle_at(pointer(false, 130.0, 12.0), cancel, 1.0, false);
        assert!(!interaction.contact.pressed);
        assert_eq!(interaction.ripple_frames_at(cancel, 1.0, false).len(), 1);

        let next_contact = start + Duration::from_millis(50);
        interaction.handle_at(pointer(true, 104.0, 30.0), next_contact, 1.0, false);
        let frames = interaction.ripple_frames_at(next_contact, 1.0, false);
        assert_eq!(frames.len(), 2);
        near(frames[1].center_x, 104.0);
        near(frames[1].center_y, 30.0);
        assert!(interaction.contact.pressed);
    }

    #[test]
    fn disabled_and_reduced_motion_events_never_leave_a_visible_ripple() {
        let start = Instant::now();
        let mut interactions = Interactions(std::collections::HashMap::new());

        interactions.handle_at(
            "disabled.button",
            pointer(true, 6.0, 10.0),
            false,
            start,
            1.0,
            false,
        );
        assert!(interactions.0.is_empty());

        interactions.handle_at(
            "reduced.button",
            pointer(true, 6.0, 10.0),
            true,
            start,
            1.0,
            true,
        );
        let reduced = &interactions.0["reduced.button"];
        assert!(reduced.contact.pressed);
        assert!(reduced.ripple_frames_at(start, 1.0, true).is_empty());
        assert!(reduced.ripples.lock().unwrap().is_empty());
    }

    #[test]
    fn logical_names_keep_interactions_independent() {
        let now = Instant::now();
        let mut interactions = Interactions(std::collections::HashMap::new());

        interactions.handle_at("first.button", pointer(true, 6.0, 10.0), true, now, 1.0, false);
        interactions.handle_at("second.button", Event::Hover(true), true, now, 1.0, false);

        let first = &interactions.0["first.button"];
        assert!(first.contact.pressed);
        assert_eq!(first.shape.target(), PRESSED_RADIUS);

        let second = &interactions.0["second.button"];
        assert!(!second.contact.pressed);
        assert!(second.contact.hovered);
        assert_eq!(second.shape.target(), REST_RADIUS);
    }
}
