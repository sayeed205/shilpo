use anyhow::{Context, Result};
use tracing::info;
use zbus::Connection;

#[zbus::proxy(
    interface = "org.gnome.SettingsDaemon.Color",
    default_service = "org.gnome.SettingsDaemon.Color",
    default_path = "/org/gnome/SettingsDaemon/Color"
)]
trait GnomeColor {
    #[zbus(property)]
    fn night_light_enabled(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn set_night_light_enabled(&self, value: bool) -> zbus::Result<()>;

    #[zbus(property)]
    fn temperature(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn set_temperature(&self, value: u32) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.kde.kwin.NightLight",
    default_service = "org.kde.KWin.NightLight",
    default_path = "/org/kde/kwin/NightLight"
)]
trait KdeNightLight {
    #[zbus(property)]
    fn enabled(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn set_enabled(&self, value: bool) -> zbus::Result<()>;

    #[zbus(property)]
    fn night_temperature(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn set_night_temperature(&self, value: u32) -> zbus::Result<()>;
}

pub struct GnomeColorBackend<'a> {
    proxy: GnomeColorProxy<'a>,
}

impl<'a> GnomeColorBackend<'a> {
    pub async fn try_init(conn: &Connection) -> Result<GnomeColorBackend<'static>> {
        let proxy = GnomeColorProxy::builder(conn)
            .build()
            .await
            .context("Failed to connect to GNOME SettingsDaemon.Color DBus proxy")?;

        // Check if property query works
        let _ = proxy
            .night_light_enabled()
            .await
            .context("GNOME Color DBus not available")?;

        info!("Successfully initialized GNOME DBus night light backend");
        Ok(GnomeColorBackend {
            proxy: proxy.to_owned(),
        })
    }

    pub async fn apply(&self, active: bool, temperature: u32) -> Result<()> {
        self.proxy.set_night_light_enabled(active).await?;
        if active {
            let _ = self.proxy.set_temperature(temperature).await;
        }
        Ok(())
    }
}

pub struct KdeNightLightBackend<'a> {
    proxy: KdeNightLightProxy<'a>,
}

impl<'a> KdeNightLightBackend<'a> {
    pub async fn try_init(conn: &Connection) -> Result<KdeNightLightBackend<'static>> {
        let proxy = KdeNightLightProxy::builder(conn)
            .build()
            .await
            .context("Failed to connect to KDE KWin.NightLight DBus proxy")?;

        let _ = proxy
            .enabled()
            .await
            .context("KDE NightLight DBus not available")?;

        info!("Successfully initialized KDE DBus night light backend");
        Ok(KdeNightLightBackend {
            proxy: proxy.to_owned(),
        })
    }

    pub async fn apply(&self, active: bool, temperature: u32) -> Result<()> {
        self.proxy.set_enabled(active).await?;
        if active {
            let _ = self.proxy.set_night_temperature(temperature).await;
        }
        Ok(())
    }
}
