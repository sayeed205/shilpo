use shilpo_ext_sdk::prelude::*;

// 1. Bar Widget Starter Shape
#[derive(Default)]
struct BarWidgetState {
    clicks: i64,
}

impl Extension for BarWidgetState {
    fn activate(&mut self, _activation: Activation) -> Result<(), Error> {
        Ok(())
    }

    fn on_event(&mut self, event: ExtensionEvent) -> Result<(), Error> {
        if let ExtensionEvent::Input(input) = event
            && input.event_id == "increment"
        {
            self.clicks += 1;
        }
        Ok(())
    }

    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
        if contribution_id != "widget" {
            return Ok(None);
        }

        Ok(Some(view! {
            row {
                icon("star").size(16.0),
                text(format!("My Widget: {}", self.clicks)).bold(true),
                button("+1", "increment"),
            }
        }))
    }
}

// 2. Desktop Widget Starter Shape
#[derive(Default)]
struct DesktopWidgetState;

impl Extension for DesktopWidgetState {
    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
        if contribution_id != "widget" {
            return Ok(None);
        }

        Ok(Some(view! {
            column {
                row {
                    icon("dashboard").size(20.0),
                    text("My Desktop").bold(true).font_size(16.0),
                },
                divider(),
                text("Desktop widget content"),
            }
        }))
    }
}

// 3. Settings Page Starter Shape
#[derive(Default)]
struct SettingsPageState {
    enabled: bool,
}

impl Extension for SettingsPageState {
    fn activate(&mut self, _activation: Activation) -> Result<(), Error> {
        self.enabled = true;
        Ok(())
    }

    fn on_event(&mut self, event: ExtensionEvent) -> Result<(), Error> {
        if let ExtensionEvent::Input(input) = event
            && input.event_id == "toggle-enabled"
        {
            self.enabled = !self.enabled;
        }
        Ok(())
    }

    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
        if contribution_id != "settings" {
            return Ok(None);
        }

        Ok(Some(view! {
            column {
                text("My Settings").bold(true).font_size(18.0),
                divider(),
                row {
                    text("Enable Feature"),
                    spacer(),
                    toggle(self.enabled, "toggle-enabled"),
                },
            }
        }))
    }
}

// 4. Side Panel Starter Shape
#[derive(Default)]
struct SidePanelState;

impl Extension for SidePanelState {
    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
        if contribution_id != "panel" {
            return Ok(None);
        }

        Ok(Some(view! {
            column {
                row {
                    icon("sidebar").size(18.0),
                    text("My Panel").bold(true),
                },
                divider(),
                text("Side panel content"),
            }
        }))
    }
}

// 5. Action Starter Shape
#[derive(Default)]
struct ActionState;

impl Extension for ActionState {
    fn activate(&mut self, _activation: Activation) -> Result<(), Error> {
        Ok(())
    }

    fn on_event(&mut self, _event: ExtensionEvent) -> Result<(), Error> {
        Ok(())
    }
}

// 6. Empty Starter Shape
#[derive(Default)]
struct EmptyState;

impl Extension for EmptyState {}

#[test]
fn test_all_six_starter_shapes() {
    // 1. Bar widget
    let mut bar = BarWidgetState::default();
    assert!(
        bar.activate(Activation {
            id: "act-1".into(),
            origin:
                shilpo_ext_sdk::bindings::shilpo::extension::types::ActivationOrigin::ShellStartup,
            extension_id: "org.example.bar".into(),
            contribution_id: None,
            input: None,
            caller: None,
            deadline: None,
        })
        .is_ok()
    );
    assert!(
        bar.on_event(ExtensionEvent::Input(InputEvent {
            contribution_id: "widget".into(),
            instance_id: None,
            event_id: "increment".into(),
            value: None,
        }))
        .is_ok()
    );
    assert_eq!(bar.clicks, 1);
    let view_tree = bar.view("widget").unwrap().expect("should return ViewTree");
    assert!(!view_tree.nodes.is_empty());

    // 2. Desktop widget
    let mut desktop = DesktopWidgetState;
    let desktop_tree = desktop.view("widget").unwrap().expect("desktop tree");
    assert!(!desktop_tree.nodes.is_empty());

    // 3. Settings page
    let mut settings = SettingsPageState::default();
    settings
        .activate(Activation {
            id: "act-2".into(),
            origin:
                shilpo_ext_sdk::bindings::shilpo::extension::types::ActivationOrigin::ShellStartup,
            extension_id: "org.example.settings".into(),
            contribution_id: None,
            input: None,
            caller: None,
            deadline: None,
        })
        .unwrap();
    assert!(settings.enabled);
    settings
        .on_event(ExtensionEvent::Input(InputEvent {
            contribution_id: "settings".into(),
            instance_id: None,
            event_id: "toggle-enabled".into(),
            value: None,
        }))
        .unwrap();
    assert!(!settings.enabled);
    let settings_tree = settings.view("settings").unwrap().expect("settings tree");
    assert!(!settings_tree.nodes.is_empty());

    // 4. Side panel
    let mut panel = SidePanelState;
    let panel_tree = panel.view("panel").unwrap().expect("panel tree");
    assert!(!panel_tree.nodes.is_empty());

    // 5. Action
    let mut action = ActionState;
    assert!(action.activate(Activation {
        id: "act-3".into(),
        origin: shilpo_ext_sdk::bindings::shilpo::extension::types::ActivationOrigin::ShellStartup,
        extension_id: "org.example.action".into(),
        contribution_id: None,
        input: None,
        caller: None,
        deadline: None,
    }).is_ok());
    assert!(action.view("none").unwrap().is_none());

    // 6. Empty
    let mut empty = EmptyState;
    assert!(empty.view("none").unwrap().is_none());
}
