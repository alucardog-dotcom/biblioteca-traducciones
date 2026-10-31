//! Jugar: con RetroBat (si lo tienes en Ajustes) se usa su lanzador, emulatorLauncher.exe, con el
//! emulador y el núcleo que tengas elegidos para ese sistema; sin RetroBat, el juego se abre con el
//! programa que Windows tenga asociado a ese tipo de fichero.

use std::fs;
use std::path::Path;
use std::process::{Child, Command};

fn valor(t: &str, nombre: &str) -> String {
    let aguja = format!("<string name=\"{nombre}\" value=\"");
    match t.find(&aguja) {
        Some(i) => {
            let resto = &t[i + aguja.len()..];
            resto[..resto.find('"').unwrap_or(0)].to_string()
        }
        None => String::new(),
    }
}

/// Emulador y núcleo como los elegiría EmulationStation: los de es_settings.cfg para ese sistema o,
/// si no hay, el predeterminado (o el primero) de es_systems.cfg.
fn emulador(es_cfg: &Path, sis: &str) -> (String, String) {
    let (mut emu, mut core) = (String::new(), String::new());
    if let Ok(t) = fs::read_to_string(es_cfg.join("es_settings.cfg")) {
        emu = valor(&t, &format!("{sis}.emulator"));
        core = valor(&t, &format!("{sis}.core"));
        for v in [&mut emu, &mut core] {
            if v == "auto" || v == "default" {
                v.clear();
            }
        }
    }
    let Ok(x) = fs::read_to_string(es_cfg.join("es_systems.cfg")) else { return (emu, core) };
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let Ok(doc) = roxmltree::Document::parse_with_options(&x, opts) else { return (emu, core) };
    let sistema = doc.descendants().find(|n| {
        n.has_tag_name("system") && n.children().any(|c| c.has_tag_name("name") && c.text().map(str::trim) == Some(sis))
    });
    let Some(s) = sistema else { return (emu, core) };
    let emus: Vec<_> = s.descendants().filter(|n| n.has_tag_name("emulator")).collect();
    let e = if emu.is_empty() {
        emus.iter().find(|n| n.attribute("default") == Some("true")).or(emus.first())
    } else {
        emus.iter().find(|n| n.attribute("name") == Some(emu.as_str()))
    };
    if let Some(e) = e {
        if emu.is_empty() {
            emu = e.attribute("name").unwrap_or("").to_string();
        }
        if core.is_empty() {
            let cores: Vec<_> = e.descendants().filter(|n| n.has_tag_name("core")).collect();
            if let Some(c) = cores.iter().find(|n| n.attribute("default") == Some("true")).or(cores.first()) {
                core = c.text().unwrap_or("").trim().to_string();
            }
        }
    }
    (emu, core)
}

/// Devuelve con qué se ha abierto ("RetroBat (emulador)" o "Windows").
pub fn abrir(retrobat: Option<&Path>, ruta: &Path, sis: &str, partida: &mut Option<Child>) -> Result<String, String> {
    if !ruta.is_file() {
        return Err("No encuentro el juego traducido.".into());
    }
    let lanzador = retrobat.map(|r| r.join("emulationstation").join("emulatorLauncher.exe")).filter(|l| l.is_file());
    let Some(lanzador) = lanzador.filter(|_| !sis.is_empty() && sis != "pc") else {
        tauri_plugin_opener::open_path(ruta, None::<&str>)
            .map_err(|_| "Windows no sabe con qué abrir este juego. Indica tu carpeta de RetroBat en Ajustes o asocia un emulador a ese tipo de fichero.".to_string())?;
        return Ok("Windows".into());
    };
    if let Some(c) = partida.as_mut() {
        if matches!(c.try_wait(), Ok(None)) {
            return Err("Ya hay un juego abierto: ciérralo antes de abrir otro.".into());
        }
    }
    let es = lanzador.parent().unwrap_or(Path::new("")).to_path_buf();
    let (emu, core) = emulador(&es.join(".emulationstation"), sis);
    let mut c = Command::new(&lanzador);
    c.arg("-system").arg(sis);
    if !emu.is_empty() {
        c.arg("-emulator").arg(&emu);
    }
    if !core.is_empty() {
        c.arg("-core").arg(&core);
    }
    c.arg("-rom").arg(ruta).current_dir(&es);
    *partida = Some(c.spawn().map_err(|e| format!("No se pudo abrir el juego: {e}"))?);
    Ok(if emu.is_empty() { "RetroBat".into() } else { format!("RetroBat ({emu})") })
}
