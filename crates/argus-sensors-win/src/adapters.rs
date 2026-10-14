//! Network adapters: names and link state from WMI, addresses from the OS interface list.

use crate::slug;
use argus_core::{Field, NetworkAdapter};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::net::IpAddr;
use sysinfo::Networks;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiAdapter {
    name: Option<String>,
    interface_description: Option<String>,
    receive_link_speed: Option<u64>,
    media_connect_state: Option<u32>,
}

/// Link speed in Mbps from the bits per second WMI reports.
fn link_mbps(speed: Option<u64>) -> Option<f64> {
    let bps = speed? as f64;
    // u64::MAX-like values mean "unknown" for virtual adapters.
    (bps > 0.0 && bps < 1e15).then_some(bps / 1e6)
}

pub fn network_adapters() -> Vec<NetworkAdapter> {
    let mut addresses: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, data) in Networks::new_with_refreshed_list().iter() {
        let ips = data
            .ip_networks()
            .iter()
            .filter_map(|n| match n.addr {
                IpAddr::V4(v4) => Some(v4.to_string()),
                IpAddr::V6(_) => None,
            })
            .collect();
        addresses.insert(name.clone(), ips);
    }

    let wmi: Option<Vec<WmiAdapter>> =
        wmi::WMIConnection::with_namespace_path("ROOT\\StandardCimv2")
            .and_then(|c| {
                c.raw_query(
            "SELECT Name, InterfaceDescription, ReceiveLinkSpeed, MediaConnectState FROM MSFT_NetAdapter",
        )
            })
            .ok();

    let mut out: Vec<NetworkAdapter> = Vec::new();
    for a in wmi.unwrap_or_default() {
        let Some(name) = a.name.filter(|n| !n.trim().is_empty()) else {
            continue;
        };
        let ips = addresses.remove(&name).unwrap_or_default();
        out.push(NetworkAdapter {
            sensor_key: slug(&name),
            description: Field::from_option(
                a.interface_description,
                "o Windows não informou a descrição",
            ),
            // 1 = connected, 2 = disconnected.
            connected: Field::from_option(
                a.media_connect_state.map(|s| s == 1),
                "o Windows não informou o estado do link",
            ),
            link_mbps: Field::from_option(
                link_mbps(a.receive_link_speed),
                "o Windows não informou a velocidade do link",
            ),
            addresses: ips,
            name,
        });
    }
    // Interfaces the OS lists but WMI did not (or WMI unavailable): keep them, with reasons.
    for (name, ips) in addresses {
        out.push(NetworkAdapter {
            sensor_key: slug(&name),
            description: Field::unavailable("o Windows (WMI) não descreveu esta interface"),
            connected: Field::unavailable("o Windows (WMI) não informou o estado do link"),
            link_mbps: Field::unavailable("o Windows (WMI) não informou a velocidade do link"),
            addresses: ips,
            name,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_speed_converts_bits_to_megabits_and_ignores_unknown() {
        assert_eq!(link_mbps(Some(1_000_000_000)), Some(1000.0));
        assert_eq!(link_mbps(Some(i64::MAX as u64)), None);
        assert_eq!(link_mbps(Some(0)), None);
        assert_eq!(link_mbps(None), None);
    }

    #[test]
    fn lists_adapters_each_with_a_stable_sensor_key() {
        let list = network_adapters();
        assert!(!list.is_empty());
        assert!(list
            .iter()
            .all(|a| !a.name.is_empty() && !a.sensor_key.is_empty()));
    }
}
