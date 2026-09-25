//! Pantallas disponibles para video (EDID)

use super::*;

/// Extrae el nombre del monitor desde el EDID binario
pub(super) fn parse_edid_name(edid: &[u8]) -> Option<String> {
    // EDID tiene 128 bytes minimo, descriptores empiezan en byte 54
    if edid.len() < 128 { return None; }
    // 4 descriptores de 18 bytes cada uno (bytes 54-125)
    for i in 0..4 {
        let offset = 54 + i * 18;
        // Monitor Name descriptor: bytes 0-2 = 0x00, byte 3 = 0xFC
        if edid[offset] == 0 && edid[offset + 1] == 0 && edid[offset + 2] == 0 && edid[offset + 3] == 0xFC {
            // El nombre esta en bytes 5-17 (13 chars), terminado por 0x0A
            let name_bytes = &edid[offset + 5..offset + 18];
            let name: String = name_bytes.iter()
                .take_while(|&&b| b != 0x0A && b != 0x00)
                .map(|&b| b as char)
                .collect();
            let name = name.trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// GET /api/music/screens - Listar pantallas conectadas con nombre del monitor
pub async fn list_screens() -> Json<Vec<ScreenInfo>> {
    let mut screens = Vec::new();
    let mut index: u8 = 0;

    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Json(screens);
    };

    let mut connectors: Vec<(String, String, bool)> = Vec::new();
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if !dir_name.contains('-') { continue; }

        let status_path = entry.path().join("status");
        let connected = std::fs::read_to_string(&status_path)
            .map(|s| s.trim() == "connected")
            .unwrap_or(false);

        if !connected { continue; }

        let connector = dir_name.split_once('-').map(|x| x.1).unwrap_or(&dir_name).to_string();

        // Leer EDID para obtener nombre del monitor
        let edid_path = entry.path().join("edid");
        let monitor_name = std::fs::read(&edid_path)
            .ok()
            .and_then(|edid| parse_edid_name(&edid))
            .unwrap_or_else(|| connector.clone());

        connectors.push((connector, monitor_name, connected));
    }

    connectors.sort_by(|a, b| a.0.cmp(&b.0));

    for (connector, name, connected) in connectors {
        screens.push(ScreenInfo { index, connector, name, connected });
        index += 1;
    }

    Json(screens)
}
