use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gpui::{App, AppContext};
use shilpo_theme_daemon::{DaemonState, ThemeClient};

use super::shell_surfaces::{self, ShellSurfaces};

static ACTIVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static CURRENT_REVISION: AtomicU64 = AtomicU64::new(0);

/// M3 Emphasized Easing: cubic-bezier(0.2, 0.0, 0.0, 1.0)
pub fn cubic_bezier_emphasized(t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let mut u = t;
    for _ in 0..8 {
        let x = 0.6 * (1.0 - u) * (1.0 - u) * u + u * u * u;
        let dx_du = 0.6 * (1.0 - u) * (1.0 - 3.0 * u) + 3.0 * u * u;
        if dx_du.abs() < 1e-6 {
            break;
        }
        let err = x - t;
        u -= err / dx_du;
        u = u.clamp(0.0, 1.0);
    }
    let y = 3.0 * (1.0 - u) * u * u + u * u * u;
    y.clamp(0.0, 1.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThemeTransition {
    pub generation: u64,
    pub from_revision: u64,
    pub to_revision: u64,
    pub start_colors: shilpo_ui::ThemeColor,
    pub target_colors: shilpo_ui::ThemeColor,
    pub target_state: DaemonState,
    pub duration_ms: u64,
}

impl ThemeTransition {
    pub fn new(
        generation: u64,
        from_revision: u64,
        to_revision: u64,
        start_colors: shilpo_ui::ThemeColor,
        target_state: DaemonState,
        duration_ms: u64,
    ) -> Self {
        let target_colors = shilpo_ui::material_theme_with_variant(
            target_state.source_argb,
            target_state.scheme_variant,
            target_state.resolved_mode.is_dark(),
        );
        Self {
            generation,
            from_revision,
            to_revision,
            start_colors,
            target_colors,
            target_state,
            duration_ms,
        }
    }

    pub fn progress_at(&self, elapsed_ms: u64) -> (f32, shilpo_ui::ThemeColor, bool) {
        if self.duration_ms == 0 || elapsed_ms >= self.duration_ms {
            (1.0, self.target_colors, true)
        } else {
            let raw_t = elapsed_ms as f32 / self.duration_ms as f32;
            let eased_t = cubic_bezier_emphasized(raw_t);
            let current_colors = self.start_colors.interpolate(&self.target_colors, eased_t);
            (eased_t, current_colors, false)
        }
    }
}

pub fn init(cx: &mut App) -> Option<PathBuf> {
    let theme_client = futures_lite::future::block_on(ThemeClient::new());
    let initial_theme_state = theme_client.current_state();
    let initial_wallpaper_path = initial_theme_state
        .wallpaper_path
        .clone()
        .filter(|path| path.is_file());
    shilpo_ui::Theme::global_mut(cx).apply_state(&initial_theme_state);
    CURRENT_REVISION.store(initial_theme_state.revision, Ordering::SeqCst);

    let mut rx = theme_client.subscribe();
    let theme_client_for_task = theme_client.clone();
    cx.spawn(async move |cx| {
        loop {
            let update = match rx.recv().await {
                Ok(update) => update,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    theme_client_for_task.current_update()
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            let state = update.state;
            let res = cx.update(|cx: &mut App| {
                let (reduced_motion, duration_ms) = {
                    let config = crate::config::ShellConfig::load_or_create(
                        crate::config::default_config_path(),
                    )
                    .unwrap_or_default();
                    (
                        config.theme.reduced_motion,
                        config.theme.transition_duration_ms,
                    )
                };

                let target_colors = shilpo_ui::material_theme_with_variant(
                    state.source_argb,
                    state.scheme_variant,
                    state.resolved_mode.is_dark(),
                );
                let start_colors = shilpo_ui::Theme::global(cx).colors;

                if reduced_motion || duration_ms == 0 {
                    ACTIVE_GENERATION.fetch_add(1, Ordering::SeqCst);
                    shilpo_ui::Theme::global_mut(cx).apply_state(&state);
                    ShellSurfaces::apply_theme_state(cx, &state);
                    super::ShellRuntime::emit_theme_signal(
                        cx,
                        state.resolved_mode.as_str().into(),
                        format!("{:?}", state.resolved_variant),
                    );
                    CURRENT_REVISION.store(state.revision, Ordering::SeqCst);
                    return None;
                }

                if target_colors == start_colors {
                    ACTIVE_GENERATION.fetch_add(1, Ordering::SeqCst);
                    shilpo_ui::Theme::global_mut(cx).apply_state(&state);
                    CURRENT_REVISION.store(state.revision, Ordering::SeqCst);
                    return None;
                }

                let generation = ACTIVE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
                let current_revision = CURRENT_REVISION.load(Ordering::SeqCst);
                Some(ThemeTransition::new(
                    generation,
                    current_revision,
                    state.revision,
                    start_colors,
                    state,
                    duration_ms,
                ))
            });

            let Some(transition) = res else {
                continue;
            };

            let generation = transition.generation;
            let span = tracing::info_span!(
                target: "shilpo_profile",
                "theme_transition",
                from_revision = transition.from_revision,
                to_revision = transition.to_revision,
                duration_ms = transition.duration_ms,
                outcome = tracing::field::Empty,
                interrupted = tracing::field::Empty,
            );

            let start_time = Instant::now();
            loop {
                if ACTIVE_GENERATION.load(Ordering::SeqCst) != generation {
                    span.record("outcome", "superseded");
                    span.record("interrupted", true);
                    break;
                }

                let elapsed_ms = start_time.elapsed().as_millis() as u64;
                let (_, current_colors, is_complete) = transition.progress_at(elapsed_ms);

                if !is_complete {
                    cx.update(|cx: &mut App| {
                        shilpo_ui::Theme::global_mut(cx).colors = current_colors;
                        cx.refresh_windows();
                    });
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                } else {
                    if ACTIVE_GENERATION.load(Ordering::SeqCst) == generation {
                        cx.update(|cx: &mut App| {
                            shilpo_ui::Theme::global_mut(cx).apply_state(&transition.target_state);
                            ShellSurfaces::apply_theme_state(cx, &transition.target_state);
                            super::ShellRuntime::emit_theme_signal(
                                cx,
                                transition.target_state.resolved_mode.as_str().into(),
                                format!("{:?}", transition.target_state.resolved_variant),
                            );
                            CURRENT_REVISION.store(
                                transition.target_state.revision,
                                Ordering::SeqCst,
                            );
                        });
                        span.record("outcome", "completed");
                        span.record("interrupted", false);
                    } else {
                        span.record("outcome", "superseded");
                        span.record("interrupted", true);
                    }
                    break;
                }
            }
        }
    })
    .detach();

    initial_wallpaper_path
}

pub fn sync_wallpaper(cx: &mut App, initial_wallpaper_path: Option<PathBuf>) {
    let wallpaper_probe =
        cx.background_spawn(async { shell_surfaces::query_awww_wallpaper_path() });
    let theme_wallpaper_path = initial_wallpaper_path;
    ThemeClient::spawn_task(async move {
        let client = ThemeClient::new().await;
        if let Some(wallpaper_path) = wallpaper_probe.await {
            let _ = client
                .set_wallpaper(&wallpaper_path.to_string_lossy())
                .await;
        } else if let Some(wallpaper_path) = theme_wallpaper_path {
            let _ = client
                .set_wallpaper(&wallpaper_path.to_string_lossy())
                .await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn theme_init_applies_daemon_state_and_reports_wallpaper() {
        let cx = gpui::TestAppContext::single();
        cx.update(|cx| shilpo_ui::init_with_source(0xFF006C4C, cx));

        let expected = futures_lite::future::block_on(ThemeClient::new());
        let expected_state = expected.current_state();
        let expected_wallpaper = expected_state
            .wallpaper_path
            .clone()
            .filter(|path| path.is_file());

        let wallpaper = cx.update(init);

        cx.update(|cx| {
            let theme = shilpo_ui::Theme::global(cx);
            assert_eq!(theme.source_argb, expected_state.source_argb);
            assert_eq!(theme.scheme_variant, expected_state.scheme_variant);
        });
        assert_eq!(wallpaper, expected_wallpaper);
    }

    #[test]
    fn test_cubic_bezier_emphasized_boundaries() {
        assert_eq!(cubic_bezier_emphasized(0.0), 0.0);
        assert_eq!(cubic_bezier_emphasized(1.0), 1.0);
        assert_eq!(cubic_bezier_emphasized(-0.5), 0.0);
        assert_eq!(cubic_bezier_emphasized(1.5), 1.0);

        let mid = cubic_bezier_emphasized(0.5);
        assert!(mid > 0.5, "M3 emphasized easing accelerates early: {mid}");
    }

    #[test]
    fn test_transition_progress_and_completion() {
        let state1 = DaemonState {
            theme: shilpo_ui::ThemeState {
                source_argb: 0xff6750a4,
                ..Default::default()
            },
            ..Default::default()
        };
        let state2 = DaemonState {
            theme: shilpo_ui::ThemeState {
                source_argb: 0xff386a20,
                ..Default::default()
            },
            ..Default::default()
        };

        let start_colors = shilpo_ui::material_theme(state1.source_argb, false);
        let transition = ThemeTransition::new(1, 1, 2, start_colors, state2.clone(), 300);

        // At t=0 ms
        let (eased, colors, complete) = transition.progress_at(0);
        assert_eq!(eased, 0.0);
        assert_eq!(colors, start_colors);
        assert!(!complete);

        // At t=150 ms (midpoint)
        let (eased_mid, colors_mid, complete_mid) = transition.progress_at(150);
        assert!(eased_mid > 0.5);
        assert_ne!(colors_mid.primary, start_colors.primary);
        assert_ne!(colors_mid.primary, transition.target_colors.primary);
        assert!(!complete_mid);

        // At t=300 ms (completion)
        let (eased_end, colors_end, complete_end) = transition.progress_at(300);
        assert_eq!(eased_end, 1.0);
        assert_eq!(colors_end, transition.target_colors);
        assert!(complete_end);
    }

    #[test]
    fn test_zero_duration_transition_is_immediate() {
        let state1 = DaemonState::default();
        let state2 = DaemonState::default();
        let start_colors = shilpo_ui::material_theme(state1.source_argb, false);
        let transition = ThemeTransition::new(1, 1, 2, start_colors, state2, 0);

        let (_, colors, complete) = transition.progress_at(0);
        assert_eq!(colors, transition.target_colors);
        assert!(complete);
    }
}
