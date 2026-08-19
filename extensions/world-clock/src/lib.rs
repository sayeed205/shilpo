#![cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
mod guest {
    use std::cell::RefCell;

    wit_bindgen::generate!({
        path: "../../core/ext-api/wit",
        world: "extension",
    });

    use shilpo::extension::{events, notifications, types, view};

    #[derive(Default)]
    struct ClockState {
        city: String,
        time: String,
    }

    thread_local! {
        static STATE: RefCell<ClockState> = RefCell::new(ClockState {
            city: "Kolkata".into(),
            time: "12:30".into(),
        });
    }

    struct WorldClock;

    impl Guest for WorldClock {
        fn activate(_activation: types::Activation) -> Result<(), types::Error> {
            Ok(())
        }

        fn deactivate(_reason: types::DeactivateReason) -> Result<(), types::Error> {
            Ok(())
        }

        fn on_event(event: events::ExtensionEvent) -> Result<(), types::Error> {
            if let events::ExtensionEvent::PaletteGenerated(_) = event {
                let _ = notifications::show(&notifications::NotificationRequest {
                    title: "World Clock".into(),
                    body: "Updated for the new palette".into(),
                    icon: None,
                });
            }
            Ok(())
        }

        fn view(contribution_id: String) -> Result<Option<view::ViewTree>, types::Error> {
            let (city, time) = STATE.with(|state| {
                let s = state.borrow();
                (s.city.clone(), s.time.clone())
            });

            let content = match contribution_id.as_str() {
                "bar" => format!("{time} · {city}"),
                "desktop" => format!("{city}\n{time}"),
                _ => return Ok(None),
            };

            let text_node = view::ViewNode::Text(view::TextNode {
                content,
                font_size: Some(14.0),
                bold: Some(true),
                style: None,
            });

            Ok(Some(view::ViewTree {
                nodes: vec![text_node],
                root: 0,
            }))
        }

        fn search(
            _contribution_id: String,
            _request: types::SearchRequest,
        ) -> Result<Vec<types::SearchCandidate>, types::Error> {
            Ok(Vec::new())
        }
    }

    export!(WorldClock);
}
