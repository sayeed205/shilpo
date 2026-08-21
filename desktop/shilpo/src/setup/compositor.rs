use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compositor {
    Niri,
}

impl fmt::Display for Compositor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Compositor::Niri => write!(f, "Niri"),
        }
    }
}

/// (menu label, selection). `None` entries are shown so users know what's coming, but
/// re-prompt instead of proceeding — there is no config to stage for them yet.
const OPTIONS: &[(&str, Option<Compositor>)] = &[
    ("Niri (recommended)", Some(Compositor::Niri)),
    ("Hyprland (coming soon)", None),
    ("Sway (coming soon)", None),
];

pub fn choose() -> Result<Compositor, String> {
    loop {
        let labels: Vec<&str> = OPTIONS.iter().map(|(label, _)| *label).collect();
        let index = dialoguer::Select::new()
            .with_prompt("Which compositor do you want Shilpo to configure?")
            .items(&labels)
            .default(0)
            .interact()
            .map_err(|e| e.to_string())?;

        match OPTIONS[index].1 {
            Some(compositor) => return Ok(compositor),
            None => {
                let name = OPTIONS[index].0.trim_end_matches(" (coming soon)");
                println!(
                    "{name} isn't supported yet — Niri is the only compositor Shilpo can configure right now.\n"
                );
            }
        }
    }
}
