use shilpo_ext_sdk::prelude::*;

struct SdkFixture {
    counter: i64,
}

impl Default for SdkFixture {
    fn default() -> Self {
        Self { counter: 42 }
    }
}

impl Extension for SdkFixture {
    fn activate(&mut self, _activation: Activation) -> Result<(), Error> {
        self.counter = 42;
        Ok(())
    }

    fn deactivate(&mut self, _reason: DeactivateReason) -> Result<(), Error> {
        Ok(())
    }

    fn on_event(&mut self, event: ExtensionEvent) -> Result<(), Error> {
        if let ExtensionEvent::Input(input) = event
            && input.event_id == "increment"
        {
            self.counter += 1;
        }
        Ok(())
    }

    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
        if contribution_id != "widget" {
            return Ok(None);
        }

        Ok(Some(view! {
            grid(2) {
                row {
                    icon("star").size(16.0),
                    text(format!("Count: {}", self.counter)).bold(true),
                },
                button("+1", "increment"),
            }
        }))
    }
}

export_extension!(SdkFixture);
