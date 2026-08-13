use std::{
    io::Cursor,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use gpui::{Context, Image, ImageFormat, Task};
use image::imageops::FilterType;

const WALLPAPER_PREVIEW_MAX_WIDTH: u32 = 960;
const WALLPAPER_PREVIEW_MAX_HEIGHT: u32 = 540;
const WALLPAPER_BLUR_SIGMA: f32 = 2.0;

fn blurred_wallpaper_preview(path: &Path) -> Option<Arc<Image>> {
    let wallpaper = image::open(path).ok()?.resize(
        WALLPAPER_PREVIEW_MAX_WIDTH,
        WALLPAPER_PREVIEW_MAX_HEIGHT,
        FilterType::Triangle,
    );
    let wallpaper = wallpaper.blur(WALLPAPER_BLUR_SIGMA);
    let mut bytes = Cursor::new(Vec::new());
    wallpaper
        .write_to(&mut bytes, image::ImageFormat::Png)
        .ok()?;
    Some(Arc::new(Image::from_bytes(
        ImageFormat::Png,
        bytes.into_inner(),
    )))
}

/// File identity used to detect same-path replacement or metadata changes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WallpaperIdentity {
    path: PathBuf,
    device: u64,
    inode: u64,
    len: u64,
    modified: Option<SystemTime>,
}

impl WallpaperIdentity {
    fn from_path(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            device: metadata.dev(),
            inode: metadata.ino(),
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}

/// The current snapshot state of the wallpaper preview resource.
#[derive(Clone, Debug)]
pub(crate) enum WallpaperPreviewSnapshot {
    Empty,
    Loading,
    Ready(Arc<Image>),
    Failed,
}

impl WallpaperPreviewSnapshot {
    pub(crate) fn ready_image(&self) -> Option<Arc<Image>> {
        match self {
            Self::Ready(image) => Some(image.clone()),
            _ => None,
        }
    }

    #[cfg(test)]
    fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    #[cfg(test)]
    fn is_failed(&self) -> bool {
        matches!(self, Self::Failed)
    }

    #[cfg(test)]
    fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

impl PartialEq for WallpaperPreviewSnapshot {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (Self::Loading, Self::Loading) => true,
            (Self::Failed, Self::Failed) => true,
            (Self::Ready(a), Self::Ready(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Deep runtime module managing wallpaper preparation, identity tracking,
/// caching, and readiness.
pub(crate) struct WallpaperPreviewResource {
    identity: Option<WallpaperIdentity>,
    snapshot: WallpaperPreviewSnapshot,
    generation: u64,
    _load_task: Option<Task<()>>,
}

impl WallpaperPreviewResource {
    pub(crate) fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            identity: None,
            snapshot: WallpaperPreviewSnapshot::Empty,
            generation: 0,
            _load_task: None,
        }
    }

    pub(crate) fn snapshot(&self) -> WallpaperPreviewSnapshot {
        self.snapshot.clone()
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.identity.as_ref().map(|id| id.path.as_path())
    }

    #[cfg(test)]
    fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn set_wallpaper_path(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(path) = path else {
            self.clear_wallpaper(cx);
            return;
        };

        let new_identity =
            WallpaperIdentity::from_path(&path).unwrap_or_else(|| WallpaperIdentity {
                path: path.clone(),
                device: 0,
                inode: 0,
                len: 0,
                modified: None,
            });

        if self.identity.as_ref() == Some(&new_identity) {
            return;
        }

        self.identity = Some(new_identity);
        self.generation += 1;
        let generation = self.generation;

        if !path.is_file() {
            log::warn!(
                "Wallpaper path does not exist or is not a file: {}",
                path.display()
            );
            self.snapshot = WallpaperPreviewSnapshot::Failed;
            self._load_task = None;
            cx.notify();
            return;
        }

        self.snapshot = WallpaperPreviewSnapshot::Loading;
        cx.notify();

        let prepare_path = path.clone();
        let load_task = cx
            .background_executor()
            .spawn(async move { blurred_wallpaper_preview(&prepare_path) });

        self._load_task = Some(cx.spawn(async move |this, cx| {
            let prepared = load_task.await;
            cx.update(|cx| {
                let Some(entity) = this.upgrade() else {
                    return;
                };
                entity.update(cx, |this, cx| {
                    if this.generation == generation {
                        if let Some(image) = prepared {
                            this.snapshot = WallpaperPreviewSnapshot::Ready(image);
                        } else {
                            log::warn!(
                                "Failed to decode/prepare wallpaper preview for {}",
                                path.display()
                            );
                            this.snapshot = WallpaperPreviewSnapshot::Failed;
                        }
                        cx.notify();
                    }
                });
            });
        }));
    }

    fn clear_wallpaper(&mut self, cx: &mut Context<Self>) {
        if self.identity.is_none() && self.snapshot == WallpaperPreviewSnapshot::Empty {
            return;
        }
        self.identity = None;
        self.snapshot = WallpaperPreviewSnapshot::Empty;
        self.generation += 1;
        self._load_task = None;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use gpui::AppContext;
    use image::RgbImage;

    use super::*;

    struct TestImageFile {
        path: PathBuf,
    }

    impl TestImageFile {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("shilpo-test-wp-{}.png", uuid::Uuid::new_v4()));
            let img = RgbImage::new(100, 100);
            img.save_with_format(&path, image::ImageFormat::Png)
                .unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestImageFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[gpui::test]
    fn unchanged_identity_does_not_reschedule(cx: &mut gpui::TestAppContext) {
        let img_file = TestImageFile::new();
        let path = img_file.path().to_path_buf();

        let resource = cx.update(|cx| cx.new(WallpaperPreviewResource::new));

        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(path.clone()), cx);
            });
        });

        cx.run_until_parked();

        let gen1 = cx.update(|cx| resource.read(cx).generation());
        let snap1 = cx.update(|cx| resource.read(cx).snapshot());
        assert!(snap1.is_ready());

        // Repeated request with same unchanged identity
        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(path.clone()), cx);
            });
        });

        let gen2 = cx.update(|cx| resource.read(cx).generation());
        assert_eq!(gen1, gen2);
    }

    #[gpui::test]
    fn path_and_metadata_change_invalidates(cx: &mut gpui::TestAppContext) {
        let img1 = TestImageFile::new();
        let img2 = TestImageFile::new();

        let resource = cx.update(|cx| cx.new(WallpaperPreviewResource::new));

        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(img1.path().to_path_buf()), cx);
            });
        });

        cx.run_until_parked();
        let snap1 = cx.update(|cx| resource.read(cx).snapshot());
        assert!(snap1.is_ready());

        // Path change
        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(img2.path().to_path_buf()), cx);
            });
        });
        let snap2_loading = cx.update(|cx| resource.read(cx).snapshot());
        assert!(snap2_loading.is_loading());

        cx.run_until_parked();
        let snap2_ready = cx.update(|cx| resource.read(cx).snapshot());
        assert!(snap2_ready.is_ready());

        // Same path metadata change (overwriting file content)
        let generation_before_metadata_change = cx.update(|cx| resource.read(cx).generation());
        std::thread::sleep(std::time::Duration::from_millis(10));
        let new_img = RgbImage::new(200, 200);
        new_img
            .save_with_format(img2.path(), image::ImageFormat::Png)
            .unwrap();

        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(img2.path().to_path_buf()), cx);
            });
        });

        cx.run_until_parked();
        let snap3_ready = cx.update(|cx| resource.read(cx).snapshot());
        assert!(snap3_ready.is_ready());
        let generation_after_metadata_change = cx.update(|cx| resource.read(cx).generation());
        assert!(generation_after_metadata_change > generation_before_metadata_change);
    }

    #[gpui::test]
    fn clearing_source_drops_ready(cx: &mut gpui::TestAppContext) {
        let img_file = TestImageFile::new();
        let resource = cx.update(|cx| cx.new(WallpaperPreviewResource::new));

        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(img_file.path().to_path_buf()), cx);
            });
        });

        cx.run_until_parked();
        assert!(cx.update(|cx| resource.read(cx).snapshot()).is_ready());

        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.clear_wallpaper(cx);
            });
        });

        let snap = cx.update(|cx| resource.read(cx).snapshot());
        assert!(snap.is_empty());
        let path = cx.update(|cx| resource.read(cx).path().map(|p| p.to_path_buf()));
        assert_eq!(path, None);
    }

    #[gpui::test]
    fn stale_completion_ignored(cx: &mut gpui::TestAppContext) {
        let img1 = TestImageFile::new();
        let img2 = TestImageFile::new();

        let resource = cx.update(|cx| cx.new(WallpaperPreviewResource::new));

        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(img1.path().to_path_buf()), cx);
                // Immediately switch to img2 before img1 finishes processing
                r.set_wallpaper_path(Some(img2.path().to_path_buf()), cx);
            });
        });

        cx.run_until_parked();
        let path = cx.update(|cx| resource.read(cx).path().map(|p| p.to_path_buf()));
        assert_eq!(path, Some(img2.path().to_path_buf()));
    }

    #[gpui::test]
    fn decode_failure_produces_failed_and_recovers(cx: &mut gpui::TestAppContext) {
        let bad_path =
            std::env::temp_dir().join(format!("shilpo-test-bad-{}.png", uuid::Uuid::new_v4()));
        std::fs::write(&bad_path, b"not an image").unwrap();

        let resource = cx.update(|cx| cx.new(WallpaperPreviewResource::new));

        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(bad_path.clone()), cx);
            });
        });

        cx.run_until_parked();
        assert!(cx.update(|cx| resource.read(cx).snapshot()).is_failed());
        let failed_generation = cx.update(|cx| resource.read(cx).generation());

        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(bad_path.clone()), cx);
            });
        });
        assert_eq!(
            cx.update(|cx| resource.read(cx).generation()),
            failed_generation,
            "a failed unchanged identity remains cached"
        );

        // Recover with valid image
        let good_file = TestImageFile::new();
        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(good_file.path().to_path_buf()), cx);
            });
        });

        cx.run_until_parked();
        assert!(cx.update(|cx| resource.read(cx).snapshot()).is_ready());

        let _ = std::fs::remove_file(bad_path);
    }

    #[gpui::test]
    fn multiple_consumers_share_same_arc(cx: &mut gpui::TestAppContext) {
        let img_file = TestImageFile::new();
        let resource = cx.update(|cx| cx.new(WallpaperPreviewResource::new));

        cx.update(|cx| {
            resource.update(cx, |r, cx| {
                r.set_wallpaper_path(Some(img_file.path().to_path_buf()), cx);
            });
        });

        cx.run_until_parked();

        let consumer1 = cx.update(|cx| resource.read(cx).snapshot());
        let consumer2 = cx.update(|cx| resource.read(cx).snapshot());

        let img1 = consumer1.ready_image().unwrap();
        let img2 = consumer2.ready_image().unwrap();

        assert!(Arc::ptr_eq(&img1, &img2));
    }
}
