wit_bindgen::generate!({
    path: "../../crates/ext/wit",
    world: "extension",
});

struct WorldClock;

impl Guest for WorldClock {
    fn on_event(event_json: String) -> String {
        let event: serde_json::Value =
            serde_json::from_str(&event_json).unwrap_or(serde_json::Value::Null);
        if event.get("kind").and_then(|kind| kind.as_str()) != Some("palette_generated") {
            return "[]".into();
        }

        serde_json::json!([
            {
                "kind": "invalidate_view",
                "contribution_id": "bar"
            },
            {
                "kind": "invalidate_view",
                "contribution_id": "desktop"
            },
            {
                "kind": "show_notification",
                "title": "World Clock",
                "body": "Updated for the new palette",
                "icon": null
            }
        ])
        .to_string()
    }

    fn view(contribution_id: String) -> String {
        let content = match contribution_id.as_str() {
            "bar" => "12:30 · Kolkata",
            "desktop" => "Kolkata\n12:30",
            _ => return "null".into(),
        };
        serde_json::json!({
            "root": {
                "kind": "text",
                "content": content,
                "font_size": 14.0,
                "bold": true,
                "style": null
            }
        })
        .to_string()
    }
}

export!(WorldClock);
