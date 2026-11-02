//! Buscar tus juegos: recorre tus carpetas, saca el CRC32 de cada ROM (sin descomprimir los .zip:
//! el zip ya lo guarda) y adivina el sistema por la extensión o por el nombre de la carpeta. También
//! encuentra los juegos de PC instalados con Steam, GOG y Epic. Lo que ya se calculó se guarda en una
//! caché, así que las siguientes búsquedas son rápidas.

use crate::datos::marcador_enlace;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub(crate) const ETIQUETA: &str = "[T-Es]";
/// Ficheros más grandes que esto (ISO de PS2, Wii…) se emparejan por nombre: leerlos entero tarda mucho.
const MAX_CRC: u64 = 1 << 30;

const POR_EXTENSION: &[(&str, &str)] = &[
    ("nes", "nes"), ("unf", "nes"), ("fds", "fds"), ("sfc", "snes"), ("smc", "snes"), ("swc", "snes"), ("fig", "snes"),
    ("bs", "satellaview"), ("gb", "gb"), ("gbc", "gbc"), ("gba", "gba"), ("n64", "n64"), ("z64", "n64"), ("v64", "n64"),
    ("nds", "nds"), ("dsi", "nds"), ("3ds", "3ds"), ("cia", "3ds"), ("vb", "virtualboy"), ("sms", "mastersystem"),
    ("gg", "gamegear"), ("sg", "sg1000"), ("md", "megadrive"), ("gen", "megadrive"), ("smd", "megadrive"), ("32x", "sega32x"),
    ("pce", "pcengine"), ("sgx", "supergrafx"), ("ngp", "ngp"), ("ngc", "ngpc"), ("ws", "wswan"), ("wsc", "wswanc"),
    ("col", "colecovision"), ("lnx", "lynx"), ("mx1", "msx1"), ("mx2", "msx2"), ("dsk", ""), ("rom", ""), ("bin", ""),
    ("nsp", "switch"), ("xci", "switch"), ("nsz", "switch"), ("xcz", "switch"), ("wua", "wiiu"), ("wux", "wiiu"),
    ("wud", "wiiu"), ("xex", "xbox360"),
];
/// Juegos de las consolas de hoy: pesan mucho y sus traducciones son mods, así que van por el nombre, sin CRC.
const SIN_CRC: &[&str] = &["nsp", "xci", "nsz", "xcz", "wua", "wux", "wud", "xex", "pkg"];
/// Discos y formatos que se emparejan por nombre (los .cue y .gdi, además, por el CRC de sus pistas).
const DISCOS: &[&str] = &["cue", "gdi", "chd", "iso", "cso", "pbp", "ccd", "cdi", "rvz", "gcm", "wbfs", "ciso", "wia", "gcz", "pkg"];
const ARCHIVOS: &[&str] = &["zip", "7z", "rar"];

/// Nombres de carpeta que dicen el sistema (en minúsculas, sin acentos).
const POR_CARPETA: &[(&str, &str)] = &[
    ("nes", "nes"), ("famicom", "nes"), ("nintendo", "nes"), ("fds", "fds"), ("famicom disk system", "fds"),
    ("snes", "snes"), ("sfc", "snes"), ("super nintendo", "snes"), ("super famicom", "snes"), ("satellaview", "satellaview"),
    ("gb", "gb"), ("game boy", "gb"), ("gameboy", "gb"), ("gbc", "gbc"), ("game boy color", "gbc"), ("gba", "gba"),
    ("game boy advance", "gba"), ("n64", "n64"), ("nintendo 64", "n64"), ("nds", "nds"), ("nintendo ds", "nds"),
    ("3ds", "3ds"), ("nintendo 3ds", "3ds"), ("virtualboy", "virtualboy"), ("virtual boy", "virtualboy"),
    ("gamecube", "gamecube"), ("gc", "gamecube"), ("wii", "wii"), ("mastersystem", "mastersystem"),
    ("master system", "mastersystem"), ("sms", "mastersystem"), ("gamegear", "gamegear"), ("game gear", "gamegear"),
    ("sg1000", "sg1000"), ("sg-1000", "sg1000"), ("megadrive", "megadrive"), ("mega drive", "megadrive"),
    ("genesis", "megadrive"), ("md", "megadrive"), ("sega32x", "sega32x"), ("32x", "sega32x"), ("megacd", "megacd"),
    ("mega cd", "megacd"), ("mega-cd", "megacd"), ("segacd", "megacd"), ("sega cd", "megacd"), ("saturn", "saturn"),
    ("dreamcast", "dreamcast"), ("dc", "dreamcast"), ("psx", "psx"), ("ps1", "psx"), ("psone", "psx"),
    ("playstation", "psx"), ("ps2", "ps2"), ("playstation 2", "ps2"), ("psp", "psp"), ("psvita", "psvita"),
    ("pcengine", "pcengine"), ("pc engine", "pcengine"), ("turbografx", "pcengine"), ("turbografx-16", "pcengine"),
    ("pcenginecd", "pcenginecd"), ("pc engine cd", "pcenginecd"), ("turbografx-cd", "pcenginecd"), ("supergrafx", "supergrafx"),
    ("pcfx", "pcfx"), ("pc-fx", "pcfx"), ("neogeocd", "neogeocd"), ("neo geo cd", "neogeocd"), ("ngp", "ngp"),
    ("ngpc", "ngpc"), ("neo geo pocket", "ngp"), ("neo geo pocket color", "ngpc"), ("wswan", "wswan"), ("wonderswan", "wswan"),
    ("wswanc", "wswanc"), ("wonderswan color", "wswanc"), ("msx", "msx1"), ("msx1", "msx1"), ("msx2", "msx2"),
    ("zxspectrum", "zxspectrum"), ("zx spectrum", "zxspectrum"), ("spectrum", "zxspectrum"), ("amstradcpc", "amstradcpc"),
    ("amstrad cpc", "amstradcpc"), ("colecovision", "colecovision"), ("lynx", "lynx"), ("3do", "3do"), ("xbox", "xbox"),
    ("pc98", "pc98"), ("pc-98", "pc98"), ("switch", "switch"), ("nintendo switch", "switch"), ("ps3", "ps3"),
    ("playstation 3", "ps3"), ("ps4", "ps4"), ("playstation 4", "ps4"), ("ps5", "ps5"), ("playstation 5", "ps5"),
    ("wiiu", "wiiu"), ("wii u", "wiiu"), ("xbox360", "xbox360"), ("xbox 360", "xbox360"),
];
pub(crate) const NO_JUEGO: &[&str] = &[
    "txt", "nfo", "diz", "doc", "docx", "pdf", "htm", "html", "url", "lnk", "xml", "cfg", "ini", "json", "png", "jpg",
    "jpeg", "gif", "bmp", "webp", "mp4", "mkv", "avi", "mp3", "ogg", "wav", "m3u", "srm", "sav", "state", "ips", "bps",
    "ups", "ppf", "xdelta", "vcdiff", "aps", "log", "md5", "sha1", "sfv", "exe", "dll", "bat", "ps1", "torrent",
];

#[derive(Default, Serialize, Deserialize)]
struct Cache(HashMap<String, (u64, u64, Vec<String>)>);

pub struct Escaner<'a> {
    cache: Cache,
    usados: HashSet<String>,
    siete_z: &'a Path,
}

pub(crate) fn ext_de(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn por_ext(e: &str) -> Option<&'static str> {
    POR_EXTENSION.iter().find(|(x, _)| *x == e).map(|(_, s)| *s)
}

/// El sistema según el nombre de las carpetas de encima (la más cercana manda).
fn por_carpeta(p: &Path, raiz: &Path) -> &'static str {
    for a in p.ancestors().skip(1) {
        if a == raiz || a.parent().is_none() {
            break;
        }
        let n = a.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if let Some((_, s)) = POR_CARPETA.iter().find(|(x, _)| *x == n) {
            return s;
        }
    }
    // La propia carpeta elegida también cuenta (p. ej. ROMs\snes).
    let n = raiz.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    POR_CARPETA.iter().find(|(x, _)| *x == n).map(|(_, s)| *s).unwrap_or("")
}

fn hex(c: u32) -> String {
    format!("{c:08X}")
}

/// CRC32 del fichero entero y, en NES y SNES, también sin la cabecera (las bases de datos no la cuentan).
fn crcs_fichero(p: &Path, sis: &str) -> Vec<String> {
    let Ok(mut f) = fs::File::open(p) else { return vec![] };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut cab = [0u8; 16];
    let n = f.read(&mut cab).unwrap_or(0);
    let salto = if n == 16 && &cab[..4] == b"NES\x1a" {
        16
    } else if (sis == "snes" || sis == "satellaview") && len % 1024 == 512 {
        512
    } else {
        0
    };
    let mut entero = crc32fast::Hasher::new();
    let mut sin = crc32fast::Hasher::new();
    entero.update(&cab[..n]);
    let mut visto = n as u64;
    if salto > 0 && salto < n as u64 {
        sin.update(&cab[salto as usize..n]);
    }
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let Ok(m) = f.read(&mut buf) else { break };
        if m == 0 {
            break;
        }
        entero.update(&buf[..m]);
        if salto > 0 {
            let desde = salto.saturating_sub(visto).min(m as u64) as usize;
            sin.update(&buf[desde..m]);
        }
        visto += m as u64;
    }
    let mut out = vec![hex(entero.finalize())];
    if salto > 0 {
        out.push(hex(sin.finalize()));
    }
    out
}

/// CRC32 de los ficheros de un .zip, leídos de su índice (sin descomprimir).
fn crcs_zip(p: &Path) -> Vec<String> {
    let Ok(f) = fs::File::open(p) else { return vec![] };
    let Ok(mut z) = zip::ZipArchive::new(f) else { return vec![] };
    (0..z.len())
        .filter_map(|i| z.by_index_raw(i).ok().map(|e| (e.name().to_string(), e.crc32(), e.is_dir())))
        .filter(|(n, _, dir)| !dir && !NO_JUEGO.contains(&ext_de(Path::new(n)).as_str()))
        .map(|(_, c, _)| hex(c))
        .collect()
}

/// CRC32 de los ficheros de un .7z o .rar, con el 7z.exe que lleva el programa.
fn crcs_7z(p: &Path, siete_z: &Path) -> Vec<String> {
    let mut c = Command::new(siete_z);
    c.args(["l", "-slt", "-ba"]).arg(p);
    #[cfg(windows)]
    c.creation_flags(0x0800_0000);
    let Ok(o) = c.output() else { return vec![] };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| l.strip_prefix("CRC = "))
        .map(|c| c.trim().to_uppercase())
        .filter(|c| c.len() == 8)
        .collect()
}

/// Las pistas que nombra un .cue o un .gdi.
pub(crate) fn pistas(hoja: &Path) -> Vec<PathBuf> {
    let Ok(t) = fs::read_to_string(hoja) else { return vec![] };
    let dir = hoja.parent().unwrap_or(Path::new(""));
    let gdi = ext_de(hoja) == "gdi";
    t.lines()
        .filter_map(|l| {
            let l = l.trim();
            if gdi {
                // "1 0 4 2352 track01.bin 0" (el nombre puede ir entre comillas).
                if let (Some(a), Some(b)) = (l.find('"'), l.rfind('"')) {
                    return (b > a).then(|| l[a + 1..b].to_string());
                }
                l.split_whitespace().nth(4).map(str::to_string)
            } else if l.to_uppercase().starts_with("FILE") {
                let (a, b) = (l.find('"')?, l.rfind('"')?);
                (b > a).then(|| l[a + 1..b].to_string())
            } else {
                None
            }
        })
        .map(|n| dir.join(n))
        .collect()
}

impl<'a> Escaner<'a> {
    pub fn new(cache: &Path, siete_z: &'a Path) -> Self {
        let cache = fs::read(cache).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Escaner { cache, usados: HashSet::new(), siete_z }
    }

    pub fn guardar(mut self, cache: &Path) {
        // Solo se quedan los ficheros que siguen existiendo.
        self.cache.0.retain(|k, _| self.usados.contains(k));
        let _ = crate::datos::escribir_atomico(cache, &serde_json::to_vec(&self.cache).unwrap_or_default());
    }

    fn crcs(&mut self, p: &Path, len: u64, sis: &str, calc: impl FnOnce(&Path) -> Vec<String>) -> Vec<String> {
        let mtime = fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        let k = format!("{}|{sis}", p.display());
        self.usados.insert(k.clone());
        if let Some((l, m, c)) = self.cache.0.get(&k) {
            if *l == len && *m == mtime {
                return c.clone();
            }
        }
        let c = calc(p);
        self.cache.0.insert(k, (len, mtime, c.clone()));
        c
    }

    /// Recorre una carpeta y añade a `juegos` lo que parezca un juego, y a `tes` lo ya traducido.
    pub fn carpeta(&mut self, raiz: &Path, juegos: &mut Vec<Value>, tes: &mut Vec<String>) {
        let mut ficheros = Vec::new();
        let mut pila = vec![raiz.to_path_buf()];
        while let Some(d) = pila.pop() {
            let Ok(it) = fs::read_dir(&d) else { continue };
            for e in it.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    if !matches!(n.as_str(), "images" | "videos" | "media" | "manuals" | "downloaded_images" | "downloaded_videos") {
                        pila.push(e.path());
                    }
                } else if ft.is_file() {
                    ficheros.push((e.path(), e.metadata().map(|m| m.len()).unwrap_or(0)));
                }
            }
        }
        // Las pistas de un .cue o .gdi no son juegos por sí solas.
        let mut de_hoja = HashSet::new();
        for (p, _) in &ficheros {
            if matches!(ext_de(p).as_str(), "cue" | "gdi") {
                de_hoja.extend(pistas(p).into_iter().map(|t| t.to_string_lossy().to_lowercase()));
            }
        }
        for (p, len) in ficheros {
            let e = ext_de(&p);
            let nombre = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if NO_JUEGO.contains(&e.as_str()) || de_hoja.contains(&p.to_string_lossy().to_lowercase()) {
                continue;
            }
            let es_disco = DISCOS.contains(&e.as_str());
            let es_archivo = ARCHIVOS.contains(&e.as_str());
            let por_e = por_ext(&e);
            if !es_disco && !es_archivo && por_e.is_none() {
                continue;
            }
            if (len < 1024 && !matches!(e.as_str(), "cue" | "gdi")) || marcador_enlace(&p, len) {
                continue;
            }
            let carpeta = por_carpeta(&p, raiz);
            let mut sis = por_e.filter(|s| !s.is_empty()).unwrap_or(carpeta).to_string();
            if nombre.contains(ETIQUETA) {
                tes.push(format!("{sis}|{nombre}|{}", p.display()));
                continue;
            }
            let (k, c) = match e.as_str() {
                "zip" => ("archive", self.crcs(&p, len, "", crcs_zip)),
                "7z" | "rar" => {
                    let sz = self.siete_z.to_path_buf();
                    ("archive", self.crcs(&p, len, "", move |p| crcs_7z(p, &sz)))
                }
                "cue" | "gdi" => {
                    let mut c = Vec::new();
                    for t in pistas(&p) {
                        let l = fs::metadata(&t).map(|m| m.len()).unwrap_or(0);
                        if l > 0 && l <= MAX_CRC {
                            c.extend(self.crcs(&t, l, "", |t| crcs_fichero(t, "")));
                        }
                    }
                    ("sheet", c)
                }
                "chd" => ("chd", vec![]),
                _ if len > MAX_CRC || SIN_CRC.contains(&e.as_str()) => ("file", vec![]),
                _ => {
                    let s = sis.clone();
                    ("file", self.crcs(&p, len, &sis, move |p| crcs_fichero(p, &s)))
                }
            };
            if es_archivo && sis.is_empty() {
                sis = carpeta.to_string();
            }
            juegos.push(json!({"s": sis, "f": p.display().to_string(), "n": nombre, "k": k, "c": c}));
        }
    }
}

// ---------------------------------------------------------------- juegos de PC instalados

fn valor_vdf(t: &str, clave: &str) -> Vec<String> {
    let aguja = format!("\"{clave}\"");
    t.lines()
        .filter_map(|l| {
            let l = l.trim();
            let resto = l.strip_prefix(&aguja)?.trim();
            let v = resto.strip_prefix('"')?.strip_suffix('"')?;
            Some(v.replace("\\\\", "\\"))
        })
        .collect()
}

fn pc(n: &str, f: &Path, k: &str, id: &str) -> Value {
    let mut v = json!({"s": "pc", "f": f.display().to_string(), "n": n, "k": k, "c": []});
    if k == "steam" {
        v["steam"] = json!(id);
    }
    v
}

#[cfg(windows)]
fn steam(juegos: &mut Vec<Value>) {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam") else { return };
    let Ok(raiz) = k.get_value::<String, _>("SteamPath") else { return };
    let raiz = PathBuf::from(raiz.replace('/', "\\"));
    let mut libs = vec![raiz.clone()];
    if let Ok(t) = fs::read_to_string(raiz.join("steamapps").join("libraryfolders.vdf")) {
        libs.extend(valor_vdf(&t, "path").into_iter().map(PathBuf::from));
    }
    let mut vistos = HashSet::new();
    for lib in libs {
        let apps = lib.join("steamapps");
        let Ok(it) = fs::read_dir(&apps) else { continue };
        for e in it.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if !n.starts_with("appmanifest_") || !n.ends_with(".acf") {
                continue;
            }
            let Ok(t) = fs::read_to_string(e.path()) else { continue };
            let (Some(id), Some(nombre), Some(dir)) =
                (valor_vdf(&t, "appid").pop(), valor_vdf(&t, "name").pop(), valor_vdf(&t, "installdir").pop())
            else {
                continue;
            };
            let lower = nombre.to_lowercase();
            if lower.contains("redistributable") || lower.starts_with("proton") || lower.starts_with("steam linux runtime") || lower == "steamvr" {
                continue;
            }
            if vistos.insert(id.clone()) {
                juegos.push(pc(&nombre, &apps.join("common").join(dir), "steam", &id));
            }
        }
    }
}

#[cfg(windows)]
fn gog(juegos: &mut Vec<Value>) {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games") else { return };
    for id in k.enum_keys().flatten() {
        let Ok(j) = k.open_subkey(&id) else { continue };
        if let (Ok(n), Ok(p)) = (j.get_value::<String, _>("gameName"), j.get_value::<String, _>("path")) {
            juegos.push(pc(&n, Path::new(&p), "gog", &id));
        }
    }
}

fn epic(juegos: &mut Vec<Value>) {
    let dir = Path::new(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(it) = fs::read_dir(dir) else { return };
    for e in it.flatten() {
        if ext_de(&e.path()) != "item" {
            continue;
        }
        let Some(v) = fs::read(e.path()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { continue };
        if let (Some(n), Some(p)) = (v["DisplayName"].as_str(), v["InstallLocation"].as_str()) {
            juegos.push(pc(n, Path::new(p), "epic", v["AppName"].as_str().unwrap_or("")));
        }
    }
}

/// Juegos de PC instalados con Steam, GOG y Epic.
pub fn tiendas(juegos: &mut Vec<Value>) {
    #[cfg(windows)]
    {
        steam(juegos);
        gog(juegos);
    }
    epic(juegos);
}
