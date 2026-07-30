use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window, div,
};
use shilpo_ui::{ActiveTheme, MediaControl, WindowExt as _, dock::PanelControl, v_flex};

use crate::section;

pub struct MediaStory {
    focus_handle: FocusHandle,
    interactive_playing: bool,
    interactive_title: String,
    interactive_artist: String,
    action_log: String,
}

impl MediaStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            interactive_playing: true,
            interactive_title: "Starboy".into(),
            interactive_artist: "The Weeknd ft. Daft Punk".into(),
            action_log: "No action yet".into(),
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for MediaStory {
    fn title() -> &'static str {
        "MediaControl"
    }

    fn description() -> &'static str {
        "MPRIS-backed, end4-inspired Material 3 media widget for horizontal and vertical status bars."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for MediaStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MediaStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity_play = cx.entity().clone();
        let entity_next = cx.entity().clone();

        v_flex()
            .w_full()
            .gap_6()
            .child(
                section("Interactive Media Control").child(
                    v_flex()
                        .gap_2()
                        .child(
                            MediaControl::new("interactive-media")
                                .title(self.interactive_title.clone())
                                .artist(self.interactive_artist.clone())
                                .playing(self.interactive_playing)
                                .can_play_pause(true)
                                .can_go_next(true)
                                .progress(0.42)
                                .on_play_pause(move |_, window, cx| {
                                    let entity = entity_play.clone();
                                    entity.update(cx, |story, cx| {
                                        story.interactive_playing = !story.interactive_playing;
                                        story.action_log = format!(
                                            "Toggled play/pause (Playing: {})",
                                            story.interactive_playing
                                        );
                                        cx.notify();
                                    });
                                    window.push_notification("Media: Play/Pause clicked", cx);
                                })
                                .on_next(move |_, window, cx| {
                                    let entity = entity_next.clone();
                                    entity.update(cx, |story, cx| {
                                        story.interactive_title = "Blinding Lights".into();
                                        story.action_log = "Skipped to next track".into();
                                        cx.notify();
                                    });
                                    window.push_notification("Media: Next clicked", cx);
                                }),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().on_surface_variant)
                                .child(format!("Action Log: {}", self.action_log)),
                        ),
                ),
            )
            .child(
                section("Playing Track with Artwork & Controls").child(
                    MediaControl::new("playing-media")
                        .title("As It Was")
                        .artist("Harry Styles")
                        .playing(true)
                        .can_play_pause(true)
                        .can_go_next(true)
                        .progress(0.65),
                ),
            )
            .child(
                section("Paused Track (Missing Artwork Fallback)").child(
                    MediaControl::new("paused-media")
                        .title("Midnight City")
                        .artist("M83")
                        .playing(false)
                        .can_play_pause(true)
                        .can_go_next(true)
                        .progress(0.20),
                ),
            )
            .child(
                section("Long Metadata (120 px Marquee)").child(
                    MediaControl::new("long-metadata-media")
                        .title("Supercalifragilisticexpialidocious Long Track Title That Exceeds Width")
                        .artist("Very Long Artist Name That Should Be Truncated Cleanly")
                        .playing(true)
                        .can_play_pause(true)
                        .can_go_next(true),
                ),
            )
            .child(
                section("Unsupported Controls (Next Hidden)").child(
                    MediaControl::new("no-next-media")
                        .title("Radio Stream")
                        .artist("Live Broadcast")
                        .playing(true)
                        .can_play_pause(true)
                        .can_go_next(false),
                ),
            )
            .child(
                section("Vertical Bar Mode (26 px Circular Toggle)").child(
                    MediaControl::new("vertical-media")
                        .title("Vertical Bar Track")
                        .artist("Vertical Artist")
                        .playing(true)
                        .vertical(true),
                ),
            )
    }
}
