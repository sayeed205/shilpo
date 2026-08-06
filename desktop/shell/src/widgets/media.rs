use gpui::{
    AnimationExt as _, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, StyledImage as _, Window, div, img, prelude::FluentBuilder as _, px,
};
use shilpo_ui::progress::ProgressCircle;
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex, v_flex};
use std::{io::Read, path::PathBuf, time::Duration};

const MAX_ARTWORK_BYTES: usize = 4 * 1024 * 1024;
const MAX_CACHED_ARTWORKS: usize = 32;

#[derive(Clone, Default)]
struct ArtworkState {
    url: String,
    path: Option<PathBuf>,
    loading: bool,
}

/// Resolves an already-cached or local album artwork URL into a file path.
/// Remote artwork is fetched by the stateful loader below, never from `RenderOnce`.
pub fn resolve_cover_art_path(art_url: &str) -> Option<PathBuf> {
    if art_url.is_empty() {
        return None;
    }

    if let Some(path_str) = art_url.strip_prefix("file://") {
        let path = PathBuf::from(path_str);
        if path.exists() {
            return Some(path);
        }
        return None;
    }

    if let Some(target_file) = cached_artwork_path(art_url) {
        return target_file.exists().then_some(target_file);
    }

    let raw_path = PathBuf::from(art_url);
    if raw_path.exists() {
        Some(raw_path)
    } else {
        None
    }
}

fn cached_artwork_path(art_url: &str) -> Option<PathBuf> {
    if !(art_url.starts_with("http://") || art_url.starts_with("https://")) {
        return None;
    }
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    art_url.hash(&mut hasher);
    let hash = hasher.finish();
    Some(
        dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("shilpo/cover_art")
            .join(format!("{hash:x}.img")),
    )
}

fn download_artwork(url: String, target: PathBuf) -> Option<PathBuf> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Shilpo Media Artwork/0.1")
        .build()
        .ok()?;
    let response = client.get(&url).send().ok()?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > MAX_ARTWORK_BYTES as u64)
    {
        return None;
    }

    let mut bytes = Vec::new();
    response
        .take((MAX_ARTWORK_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_ARTWORK_BYTES {
        return None;
    }

    let cache_dir = target.parent()?;
    std::fs::create_dir_all(cache_dir).ok()?;
    let temporary = target.with_extension("tmp");
    std::fs::write(&temporary, bytes).ok()?;
    std::fs::rename(&temporary, &target).ok()?;

    let mut entries = std::fs::read_dir(cache_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "img"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(modified, _)| *modified);
    while entries.len() > MAX_CACHED_ARTWORKS {
        let (_, path) = entries.remove(0);
        let _ = std::fs::remove_file(path);
    }
    Some(target)
}

/// A presentational MPRIS-backed Material 3 media control widget.
#[derive(IntoElement)]
pub struct MediaControl {
    id: ElementId,
    title: SharedString,
    artist: SharedString,
    art_url: SharedString,
    is_playing: bool,
    can_play_pause: bool,
    can_go_next: bool,
    progress: f32,
    vertical: bool,
    reduced_motion: bool,
    #[allow(clippy::type_complexity)]
    on_play_pause: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    #[allow(clippy::type_complexity)]
    on_next: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    style: StyleRefinement,
}

impl MediaControl {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            title: SharedString::default(),
            artist: SharedString::default(),
            art_url: SharedString::default(),
            is_playing: false,
            can_play_pause: true,
            can_go_next: true,
            progress: 0.0,
            vertical: false,
            reduced_motion: false,
            on_play_pause: None,
            on_next: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    pub fn artist(mut self, artist: impl Into<SharedString>) -> Self {
        self.artist = artist.into();
        self
    }

    pub fn art_url(mut self, art_url: impl Into<SharedString>) -> Self {
        self.art_url = art_url.into();
        self
    }

    pub fn playing(mut self, playing: bool) -> Self {
        self.is_playing = playing;
        self
    }

    pub fn can_play_pause(mut self, can_play_pause: bool) -> Self {
        self.can_play_pause = can_play_pause;
        self
    }

    pub fn can_go_next(mut self, can_go_next: bool) -> Self {
        self.can_go_next = can_go_next;
        self
    }

    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress.clamp(0.0, 1.0);
        self
    }

    pub fn vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    pub fn reduced_motion(mut self, reduced_motion: bool) -> Self {
        self.reduced_motion = reduced_motion;
        self
    }

    pub fn on_play_pause(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_play_pause = Some(Box::new(listener));
        self
    }

    pub fn on_next(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_next = Some(Box::new(listener));
        self
    }
}

impl Styled for MediaControl {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaControl {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let art_url = self.art_url.to_string();
        let artwork_state =
            _window.use_keyed_state(format!("media-artwork-{}", self.id), cx, |_, _| {
                ArtworkState::default()
            });
        let artwork_snapshot = artwork_state.read(cx).clone();
        if artwork_snapshot.url != art_url {
            let local_path = resolve_cover_art_path(&art_url)
                .or_else(|| cached_artwork_path(&art_url).filter(|path| path.exists()));
            artwork_state.update(cx, |state, cx| {
                state.url = art_url.clone();
                state.path = local_path.clone();
                state.loading = local_path.is_none()
                    && (art_url.starts_with("http://") || art_url.starts_with("https://"));
                cx.notify();
            });
            if local_path.is_none()
                && (art_url.starts_with("http://") || art_url.starts_with("https://"))
                && let Some(target) = cached_artwork_path(&art_url)
            {
                let state = artwork_state.clone();
                let requested_url = art_url.clone();
                cx.spawn(async move |cx| {
                    let download_url = requested_url.clone();
                    let downloaded = cx
                        .background_executor()
                        .spawn(async move { download_artwork(download_url, target) })
                        .await;
                    state.update(cx, |state, cx| {
                        if state.url == requested_url {
                            state.loading = false;
                            state.path = downloaded;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }
        let artwork_path = artwork_snapshot.path;
        let reduced_motion = self.reduced_motion;

        let inner_thumb = if let Some(path) = artwork_path {
            div()
                .size(px(24.))
                .rounded_full()
                .overflow_hidden()
                .child(
                    img(path)
                        .size(px(24.))
                        .rounded_full()
                        .object_fit(ObjectFit::Cover),
                )
                .into_any_element()
        } else {
            h_flex()
                .size(px(24.))
                .rounded_full()
                .bg(cx.theme().secondary_container)
                .justify_center()
                .child(
                    Icon::new(IconName::MusicNote)
                        .size(px(12.))
                        .text_color(cx.theme().on_secondary_container),
                )
                .into_any_element()
        };

        let hover_icon = if self.is_playing {
            IconName::PauseFill
        } else {
            IconName::PlayArrowFill
        };

        let progress_ring = ProgressCircle::new(format!("media-progress-ring-{}", self.id))
            .size(px(30.))
            .stroke_width(px(2.0))
            .value(self.progress * 100.0)
            .wavy(self.is_playing && !reduced_motion)
            .wave_speed(0.5)
            .color(cx.theme().primary)
            .child(inner_thumb);

        let mut artwork_element = h_flex()
            .group("media_thumb_group")
            .id(format!("{}-play-pause", self.id))
            .size(px(30.))
            .rounded_full()
            .relative()
            .justify_center()
            .items_center()
            .child(progress_ring)
            .when(self.can_play_pause, |this| {
                this.cursor_pointer()
                    .role(Role::Button)
                    .aria_label(if self.is_playing {
                        "Pause media"
                    } else {
                        "Play media"
                    })
            })
            .when(self.can_play_pause, |this| {
                this.child(
                    h_flex()
                        .absolute()
                        .inset_0()
                        .rounded_full()
                        .bg(cx.theme().surface.opacity(0.55))
                        .opacity(0.0)
                        .group_hover("media_thumb_group", |style| style.opacity(1.0))
                        .justify_center()
                        .items_center()
                        .child(
                            Icon::new(hover_icon)
                                .size(px(12.))
                                .text_color(cx.theme().on_surface),
                        ),
                )
            });

        if self.can_play_pause
            && let Some(on_play_pause) = self.on_play_pause
        {
            artwork_element = artwork_element.on_click(on_play_pause);
        }

        if self.vertical {
            return artwork_element;
        }

        // Horizontal Layout (end4-inspired Material widget)
        let artist_display = if self.artist.is_empty() {
            SharedString::from("Unknown Artist")
        } else {
            self.artist.clone()
        };

        let title_display = if self.title.is_empty() {
            SharedString::from("Unknown Title")
        } else {
            self.title.clone()
        };

        let title_len = title_display.chars().count();
        let title_element = if title_len > 14 && !reduced_motion {
            let full_text = format!("{}   •   {}", title_display, title_display);
            let scroll_dist = (title_len as f32 + 7.0) * 6.5;

            div()
                .overflow_hidden()
                .max_w(px(110.))
                .child(
                    div()
                        .flex()
                        .whitespace_nowrap()
                        .text_xs()
                        .font_medium()
                        .text_color(cx.theme().on_surface)
                        .child(full_text)
                        .with_animation(
                            format!("media-title-marquee-{}", self.id),
                            gpui::Animation::new(Duration::from_secs(22)).repeat(),
                            move |this, delta| {
                                let shift = if delta < 0.25 {
                                    0.0
                                } else {
                                    let p = (delta - 0.25) / 0.75;
                                    let ease =
                                        shilpo_ui::animation::cubic_bezier(0.25, 0.1, 0.25, 1.0)(p);
                                    ease * scroll_dist
                                };
                                this.ml(px(-shift))
                            },
                        ),
                )
                .into_any_element()
        } else {
            div()
                .text_xs()
                .font_medium()
                .text_color(cx.theme().on_surface)
                .truncate()
                .child(title_display)
                .into_any_element()
        };

        let next_bg = cx.theme().secondary_container;
        let next_fg = cx.theme().on_secondary_container;
        let next_hover_bg = cx.theme().primary.opacity(0.25);

        let mut next_btn = h_flex()
            .id(format!("{}-next", self.id))
            .flex_none()
            .size(px(26.))
            .rounded_full()
            .bg(next_bg)
            .hover(|style| style.bg(next_hover_bg))
            .cursor_pointer()
            .role(Role::Button)
            .aria_label("Next media")
            .justify_center()
            .child(
                Icon::new(IconName::SkipNextFill)
                    .size(px(14.))
                    .text_color(next_fg),
            );

        if self.can_go_next
            && let Some(on_next) = self.on_next
        {
            next_btn = next_btn.on_click(on_next);
        }

        let metadata_column = v_flex()
            .flex_1()
            .max_w(px(110.))
            .justify_center()
            .gap(px(0.))
            .child(
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().on_surface)
                    .truncate()
                    .child(artist_display),
            )
            .child(title_element);

        h_flex()
            .id(self.id)
            .max_w(px(200.))
            .items_center()
            .gap(px(6.))
            .child(artwork_element)
            .child(metadata_column)
            .when(self.can_go_next, |this| this.child(next_btn))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_cover_art_empty() {
        assert_eq!(resolve_cover_art_path(""), None);
    }

    #[test]
    fn test_resolve_cover_art_nonexistent_file() {
        assert_eq!(
            resolve_cover_art_path("file:///nonexistent/path/art.png"),
            None
        );
    }

    #[test]
    fn test_media_control_builder() {
        let control = MediaControl::new("test-media")
            .title("Test Title")
            .artist("Test Artist")
            .playing(true)
            .can_play_pause(true)
            .can_go_next(false)
            .progress(0.45)
            .vertical(true);

        assert_eq!(control.title, "Test Title");
        assert_eq!(control.artist, "Test Artist");
        assert!(control.is_playing);
        assert!(control.can_play_pause);
        assert!(!control.can_go_next);
        assert_eq!(control.progress, 0.45);
        assert!(control.vertical);
    }
}
