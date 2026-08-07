use super::{IpConfig, VpnConnection, WifiAccessPoint};
use anyhow::{Context, Result};
use std::collections::HashMap;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};
use zbus::Connection;

const NM_BUS_NAME: &str = "org.freedesktop.NetworkManager";
const NM_OBJECT_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_IFACE: &str = "org.freedesktop.NetworkManager";
const NM_SETTINGS_OBJECT_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const NM_SETTINGS_IFACE: &str = "org.freedesktop.NetworkManager.Settings";
const NM_SETTINGS_CONN_IFACE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const NM_ACTIVE_CONN_IFACE: &str = "org.freedesktop.NetworkManager.ActiveConnection";
const NM_DEVICE_IFACE: &str = "org.freedesktop.NetworkManager.Device";
const NM_WIFI_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const NM_AP_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
const NM_IP4_IFACE: &str = "org.freedesktop.NetworkManager.IP4Config";
const DBUS_PROP_IFACE: &str = "org.freedesktop.DBus.Properties";

pub async fn get_property<T: zbus::zvariant::Type + serde::de::DeserializeOwned>(
    conn: &Connection,
    path: &str,
    iface: &str,
    prop: &str,
) -> Result<T> {
    let obj_path = ObjectPath::try_from(path)?;
    let reply = conn
        .call_method(
            Some(NM_BUS_NAME),
            obj_path,
            Some(DBUS_PROP_IFACE),
            "Get",
            &(iface, prop),
        )
        .await?;
    let val: T = reply.body().deserialize()?;
    Ok(val)
}

pub async fn set_property<'a>(
    conn: &Connection,
    path: &str,
    iface: &str,
    prop: &str,
    val: Value<'a>,
) -> Result<()> {
    let obj_path = ObjectPath::try_from(path)?;
    conn.call_method(
        Some(NM_BUS_NAME),
        obj_path,
        Some(DBUS_PROP_IFACE),
        "Set",
        &(iface, prop, val),
    )
    .await?;
    Ok(())
}

pub async fn get_nm_state(conn: &Connection) -> Result<u32> {
    let reply = conn
        .call_method(
            Some(NM_BUS_NAME),
            NM_OBJECT_PATH,
            Some(NM_IFACE),
            "state",
            &(),
        )
        .await?;
    let state: u32 = reply.body().deserialize()?;
    Ok(state)
}

pub async fn get_wireless_enabled(conn: &Connection) -> Result<bool> {
    get_property::<bool>(conn, NM_OBJECT_PATH, NM_IFACE, "WirelessEnabled").await
}

pub async fn set_wireless_enabled(conn: &Connection, enabled: bool) -> Result<()> {
    set_property(
        conn,
        NM_OBJECT_PATH,
        NM_IFACE,
        "WirelessEnabled",
        Value::Bool(enabled),
    )
    .await
}

pub async fn set_wwan_enabled(conn: &Connection, enabled: bool) -> Result<()> {
    set_property(
        conn,
        NM_OBJECT_PATH,
        NM_IFACE,
        "WwanEnabled",
        Value::Bool(enabled),
    )
    .await
}

pub async fn deactivate_connection(conn: &Connection, active_path: &str) -> Result<()> {
    let obj_path = ObjectPath::try_from(active_path)?;
    conn.call_method(
        Some(NM_BUS_NAME),
        NM_OBJECT_PATH,
        Some(NM_IFACE),
        "DeactivateConnection",
        &(obj_path,),
    )
    .await?;
    Ok(())
}

pub async fn request_wifi_scan(conn: &Connection) -> Result<()> {
    let devices: Vec<OwnedObjectPath> =
        get_property(conn, NM_OBJECT_PATH, NM_IFACE, "AllDevices").await?;

    for dev_path in devices {
        let dev_type: u32 =
            match get_property(conn, dev_path.as_str(), NM_DEVICE_IFACE, "DeviceType").await {
                Ok(t) => t,
                Err(_) => continue,
            };

        // DeviceType 2 == Wi-Fi
        if dev_type == 2 {
            let options: HashMap<String, Value> = HashMap::new();
            let _ = conn
                .call_method(
                    Some(NM_BUS_NAME),
                    dev_path.as_str(),
                    Some(NM_WIFI_IFACE),
                    "RequestScan",
                    &(options,),
                )
                .await;
        }
    }
    Ok(())
}

pub async fn list_access_points(conn: &Connection) -> Result<Vec<WifiAccessPoint>> {
    let mut access_points = Vec::new();
    let devices: Vec<OwnedObjectPath> =
        get_property(conn, NM_OBJECT_PATH, NM_IFACE, "AllDevices").await?;

    for dev_path in devices {
        let dev_type: u32 =
            match get_property(conn, dev_path.as_str(), NM_DEVICE_IFACE, "DeviceType").await {
                Ok(t) => t,
                Err(_) => continue,
            };

        if dev_type == 2 {
            let ap_paths: Vec<OwnedObjectPath> =
                match get_property(conn, dev_path.as_str(), NM_WIFI_IFACE, "AccessPoints").await {
                    Ok(paths) => paths,
                    Err(_) => continue,
                };

            let active_ap_path: Option<OwnedObjectPath> =
                get_property(conn, dev_path.as_str(), NM_WIFI_IFACE, "ActiveAccessPoint")
                    .await
                    .ok();

            for ap_path in ap_paths {
                let ap_str = ap_path.as_str();
                let raw_ssid: Vec<u8> = get_property(conn, ap_str, NM_AP_IFACE, "Ssid")
                    .await
                    .unwrap_or_default();
                let ssid = String::from_utf8_lossy(&raw_ssid).to_string();
                if ssid.trim().is_empty() {
                    continue;
                }

                let bssid: String = get_property(conn, ap_str, NM_AP_IFACE, "HwAddress")
                    .await
                    .unwrap_or_default();
                let strength: u8 = get_property(conn, ap_str, NM_AP_IFACE, "Strength")
                    .await
                    .unwrap_or(0);
                let freq: u32 = get_property(conn, ap_str, NM_AP_IFACE, "Frequency")
                    .await
                    .unwrap_or(0);

                let wpa_flags: u32 = get_property(conn, ap_str, NM_AP_IFACE, "WpaFlags")
                    .await
                    .unwrap_or(0);
                let rsn_flags: u32 = get_property(conn, ap_str, NM_AP_IFACE, "RsnFlags")
                    .await
                    .unwrap_or(0);

                let security_type = if rsn_flags != 0 {
                    "WPA2/WPA3".to_string()
                } else if wpa_flags != 0 {
                    "WPA".to_string()
                } else {
                    "Open".to_string()
                };

                let is_connected = active_ap_path.as_ref() == Some(&ap_path);

                access_points.push(WifiAccessPoint {
                    ssid,
                    bssid,
                    signal_percent: strength,
                    security_type,
                    frequency_mhz: freq,
                    is_connected,
                    object_path: ap_str.to_string(),
                });
            }
        }
    }

    access_points.sort_by_key(|ap| std::cmp::Reverse(ap.signal_percent));
    Ok(access_points)
}

pub async fn list_active_vpns(conn: &Connection) -> Result<Vec<VpnConnection>> {
    let active_paths: Vec<OwnedObjectPath> =
        get_property(conn, NM_OBJECT_PATH, NM_IFACE, "ActiveConnections").await?;

    let mut vpns = Vec::new();
    for active_path in active_paths {
        let path_str = active_path.as_str();
        let conn_type: String = get_property(conn, path_str, NM_ACTIVE_CONN_IFACE, "Type")
            .await
            .unwrap_or_default();

        if conn_type == "vpn" || conn_type.contains("wireguard") || conn_type.contains("openvpn") {
            let id: String = get_property(conn, path_str, NM_ACTIVE_CONN_IFACE, "Id")
                .await
                .unwrap_or_default();
            let uuid: String = get_property(conn, path_str, NM_ACTIVE_CONN_IFACE, "Uuid")
                .await
                .unwrap_or_default();
            let state: u32 = get_property(conn, path_str, NM_ACTIVE_CONN_IFACE, "State")
                .await
                .unwrap_or(0);

            // State 2 == NM_ACTIVE_CONNECTION_STATE_ACTIVATED
            let is_active = state == 2;

            vpns.push(VpnConnection {
                id,
                uuid,
                vpn_type: conn_type,
                is_active,
                object_path: path_str.to_string(),
            });
        }
    }
    Ok(vpns)
}

pub async fn connect_vpn(conn: &Connection, name_or_uuid: &str) -> Result<()> {
    let list_reply = conn
        .call_method(
            Some(NM_BUS_NAME),
            NM_SETTINGS_OBJECT_PATH,
            Some(NM_SETTINGS_IFACE),
            "ListConnections",
            &(),
        )
        .await?;
    let conn_paths: Vec<OwnedObjectPath> = list_reply.body().deserialize()?;

    let mut target_setting_path: Option<OwnedObjectPath> = None;

    for setting_path in conn_paths {
        let reply = conn
            .call_method(
                Some(NM_BUS_NAME),
                setting_path.as_str(),
                Some(NM_SETTINGS_CONN_IFACE),
                "GetSettings",
                &(),
            )
            .await;

        if let Ok(reply) = reply
            && let Ok(settings) =
                reply.body().deserialize::<HashMap<String, HashMap<String, Value>>>()
            && let Some(conn_setting) = settings.get("connection")
        {
            let id = conn_setting
                .get("id")
                .and_then(|v| String::try_from(v.clone()).ok());
            let uuid = conn_setting
                .get("uuid")
                .and_then(|v| String::try_from(v.clone()).ok());

            if id.as_deref() == Some(name_or_uuid) || uuid.as_deref() == Some(name_or_uuid) {
                target_setting_path = Some(setting_path);
                break;
            }
        }
    }

    let target_path = target_setting_path
        .context(format!("VPN connection profile '{name_or_uuid}' not found"))?;

    let root_path1 = ObjectPath::try_from("/")?;
    let root_path2 = ObjectPath::try_from("/")?;
    conn.call_method(
        Some(NM_BUS_NAME),
        NM_OBJECT_PATH,
        Some(NM_IFACE),
        "ActivateConnection",
        &(target_path.as_ref(), root_path1, root_path2),
    )
    .await?;

    Ok(())
}

pub async fn disconnect_vpn(conn: &Connection, name_or_path: &str) -> Result<()> {
    let vpns = list_active_vpns(conn).await?;
    for vpn in vpns {
        if vpn.id == name_or_path || vpn.uuid == name_or_path || vpn.object_path == name_or_path {
            deactivate_connection(conn, &vpn.object_path).await?;
            return Ok(());
        }
    }
    anyhow::bail!("Active VPN '{name_or_path}' not found")
}

pub async fn get_primary_connection_info(conn: &Connection) -> Result<(String, Option<IpConfig>)> {
    let active_paths: Vec<OwnedObjectPath> =
        get_property(conn, NM_OBJECT_PATH, NM_IFACE, "ActiveConnections").await?;

    let mut connection_type = "none".to_string();
    let mut ip_config = None;

    for active_path in active_paths {
        let path_str = active_path.as_str();
        let conn_type: String = get_property(conn, path_str, NM_ACTIVE_CONN_IFACE, "Type")
            .await
            .unwrap_or_default();
        let state: u32 = get_property(conn, path_str, NM_ACTIVE_CONN_IFACE, "State")
            .await
            .unwrap_or(0);

        if state == 2 && conn_type != "vpn" {
            connection_type = conn_type.clone();

            let ip4_path: OwnedObjectPath =
                get_property(conn, path_str, NM_ACTIVE_CONN_IFACE, "Ip4Config")
                    .await
                    .unwrap_or_else(|_| OwnedObjectPath::try_from("/").unwrap());

            if ip4_path.as_str() != "/" {
                let gateway: Option<String> =
                    get_property(conn, ip4_path.as_str(), NM_IP4_IFACE, "Gateway")
                        .await
                        .ok();
                ip_config = Some(IpConfig {
                    ipv4_address: None,
                    ipv4_gateway: gateway,
                    dns_servers: Vec::new(),
                });
            }
            break;
        }
    }

    Ok((connection_type, ip_config))
}
