//! Aplicar una traducción a tu juego: lo que hacía Parchear.ps1 con -Juego, ya sin PowerShell.
//!
//! El parche puede venir suelto o dentro de un .zip/.7z/.rar, y el juego puede ser una ROM (suelta o
//! comprimida), un disco .cue/.gdi con sus pistas o un .chd. Se comprueba que el juego es la versión que
//! pide la traducción (por CRC32), se aplica con el motor (motor.rs) o con xdelta3.exe y el traducido se
//! deja con la etiqueta [T-Es] junto al original o en tu carpeta de traducidos. El original nunca se toca.

use crate::escaneo::{ext_de, pistas, ETIQUETA, NO_JUEGO};
use crate::motor::{self, Comprobacion, Formato};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const SISTEMAS_SNES: [&str; 5] = ["snes", "snes-msu1", "satellaview", "sufami", "sgb"];
const ARCHIVOS: [&str; 3] = ["zip", "7z", "rar"];
/// Formatos que van comprimidos a su manera: hay que convertirlos antes.
const SIN_PARCHEAR: [(&str, &str); 7] = [("cso", "CSO"), ("rvz", "RVZ"), ("wbfs", "WBFS"), ("pbp", "PBP"), ("ecm", "ECM"), ("wia", "WIA"), ("gcz", "GCZ")];
/// Hasta este tamaño se calcula el CRC32 para comprobar la versión (más grandes, tarda demasiado).
const MAX_CRC: u64 = 128 * 1024 * 1024;
const MAX_CRC_COMPRIMIDO: u64 = 16 * 1024 * 1024;

/// Dónde están las herramientas y dónde trabajar.
pub struct Entorno<'a> {
    pub siete_z: &'a Path,
    pub chdman: &'a Path,
    pub xdelta: &'a Path,
    pub temp: &'a Path,
    pub hechos: &'a Path,
    /// "junto": el traducido va al lado del original. Si es una carpeta, a <carpeta>\<sistema>.
    pub salida: &'a str,
}

// ---------------------------------------------------------------- herramientas

/// Ejecuta 7z, chdman o xdelta3 sin ventana. Devuelve si acabó bien y lo que escribió.
pub fn herramienta<I, S>(exe: &Path, args: I) -> Result<(bool, String), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if !exe.is_file() {
        return Err(format!("Falta {} en las herramientas del programa.", exe.file_name().unwrap_or_default().to_string_lossy()));
    }
    let mut c = Command::new(exe);
    c.args(args);
    #[cfg(windows)]
    c.creation_flags(0x0800_0000);
    let o = c.output().map_err(|e| format!("No se pudo ejecutar {}: {e}", exe.display()))?;
    let mut texto = String::from_utf8_lossy(&o.stdout).into_owned();
    texto.push_str(&String::from_utf8_lossy(&o.stderr));
    Ok((o.status.success(), texto))
}

static CONTADOR: AtomicU32 = AtomicU32::new(0);

/// Carpeta de trabajo que se borra sola al terminar.
pub struct Temporal(pub PathBuf);

impl Temporal {
    pub fn nueva(dentro: &Path) -> Result<Temporal, String> {
        let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let d = dentro.join(format!("{:x}{:x}", ns & 0xffff_ffff_ffff, CONTADOR.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&d).map_err(|e| format!("No se pudo crear la carpeta temporal: {e}"))?;
        Ok(Temporal(d))
    }
}

impl Drop for Temporal {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Mueve un fichero, también a otra unidad (donde `rename` no sirve).
pub fn mover(de: &Path, a: &Path) -> Result<(), String> {
    if fs::rename(de, a).is_ok() {
        return Ok(());
    }
    fs::copy(de, a).map_err(|e| format!("No se pudo guardar {}: {e}", a.display()))?;
    let _ = fs::remove_file(de);
    Ok(())
}

/// Un fichero dentro de un comprimido.
#[derive(Clone, Debug)]
pub struct Entrada {
    pub ruta: String,
    pub tam: u64,
    pub crc: String,
}

/// Lo que hay dentro de un .zip/.7z/.rar, sin descomprimirlo (7z guarda el CRC32 de cada fichero).
pub fn listar(siete_z: &Path, archivo: &Path) -> Vec<Entrada> {
    let Ok((true, t)) = herramienta(siete_z, [OsStr::new("l"), OsStr::new("-slt"), OsStr::new("-ba"), OsStr::new("-sccUTF-8"), archivo.as_os_str()]) else {
        return vec![];
    };
    let mut out: Vec<(Entrada, bool)> = vec![];
    for l in t.lines() {
        if let Some(r) = l.strip_prefix("Path = ") {
            out.push((Entrada { ruta: r.to_string(), tam: 0, crc: String::new() }, false));
        } else if let Some((e, dir)) = out.last_mut() {
            if let Some(n) = l.strip_prefix("Size = ") {
                e.tam = n.trim().parse().unwrap_or(0);
            } else if let Some(c) = l.strip_prefix("CRC = ") {
                e.crc = c.trim().to_uppercase();
            } else if l.starts_with("Folder = +") || l.starts_with("Attributes = D") {
                *dir = true;
            }
        }
    }
    out.into_iter().filter(|(_, dir)| !dir).map(|(e, _)| e).collect()
}

/// Descomprime todo o solo `entrada` en `dest`.
pub fn descomprimir(siete_z: &Path, archivo: &Path, dest: &Path, entrada: Option<&str>) -> Result<(), String> {
    let mut args = vec![OsStr::new("x").to_owned(), "-y".into(), "-spd".into()];
    let mut o = std::ffi::OsString::from("-o");
    o.push(dest);
    args.push(o);
    args.push(archivo.as_os_str().to_owned());
    if let Some(e) = entrada {
        args.push(e.into());
    }
    match herramienta(siete_z, &args)? {
        (true, _) => Ok(()),
        _ => Err(format!("No se pudo descomprimir {}.", nombre_de(archivo))),
    }
}

fn nombre_de(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn base_de(p: &Path) -> String {
    p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Todos los ficheros de una carpeta y sus subcarpetas, en orden.
fn ficheros(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let mut pend = vec![dir.to_path_buf()];
    while let Some(d) = pend.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                pend.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------- parches

pub struct Parche {
    pub fichero: PathBuf,
    pub formato: Formato,
    /// El fichero tal como se descargó (el comprimido, si venía dentro de uno).
    pub original: PathBuf,
}

/// Los parches que hay en `archivo`: él mismo o los que vengan dentro si es un comprimido.
fn parches_de(e: &Entorno, archivo: &Path, trabajo: &Path) -> Result<Vec<Parche>, String> {
    if ARCHIVOS.contains(&ext_de(archivo).as_str()) {
        let dest = trabajo.join("parches");
        descomprimir(e.siete_z, archivo, &dest, None)?;
        return Ok(ficheros(&dest)
            .into_iter()
            .filter_map(|f| motor::detectar(&f).map(|formato| Parche { fichero: f, formato, original: archivo.to_path_buf() }))
            .collect());
    }
    Ok(motor::detectar(archivo).map(|formato| Parche { fichero: archivo.to_path_buf(), formato, original: archivo.to_path_buf() }).into_iter().collect())
}

// ---------------------------------------------------------------- tu juego

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tipo {
    Fichero,
    Comprimido,
    Hoja,
    Chd,
}

struct Candidato {
    ruta: PathBuf,
    tipo: Tipo,
    nombre: String,
    entradas: Option<Vec<Entrada>>,
}

impl Candidato {
    fn de(ruta: &Path) -> Candidato {
        let tipo = match ext_de(ruta).as_str() {
            "zip" | "7z" | "rar" => Tipo::Comprimido,
            "cue" | "gdi" => Tipo::Hoja,
            "chd" => Tipo::Chd,
            _ => Tipo::Fichero,
        };
        Candidato { ruta: ruta.to_path_buf(), tipo, nombre: base_de(ruta), entradas: None }
    }

    fn entradas(&mut self, siete_z: &Path) -> &[Entrada] {
        if self.entradas.is_none() {
            self.entradas = Some(listar(siete_z, &self.ruta));
        }
        self.entradas.as_deref().unwrap_or_default()
    }
}

/// Un CRC32 de tu juego: el del fichero entero o sin la cabecera (`salto` bytes), o el de una pista o un
/// fichero de dentro del comprimido (`entrada`).
struct CrcJuego {
    crc: String,
    salto: u64,
    entrada: Option<String>,
}

/// Bytes de cabecera que no cuentan en el CRC de las bases de datos: 16 en NES (iNES) y 512 en las ROMs
/// de SNES con cabecera de copiador.
fn cabecera(p: &Path, sis: &str) -> u64 {
    let len = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let mut b = [0u8; 4];
    let nes = fs::File::open(p).and_then(|mut f| f.read_exact(&mut b)).is_ok() && &b == b"NES\x1a";
    if nes {
        16
    } else if SISTEMAS_SNES.contains(&sis) && len % 1024 == 512 {
        512
    } else {
        0
    }
}

fn crcs_de_fichero(p: &Path, sis: &str, out: &mut Vec<CrcJuego>) {
    let cab = cabecera(p, sis);
    for salto in if cab > 0 { vec![0, cab] } else { vec![0] } {
        if let Ok(c) = motor::crc_fichero(p, salto) {
            out.push(CrcJuego { crc: motor::hex(c), salto, entrada: None });
        }
    }
}

fn crcs_juego(e: &Entorno, c: &mut Candidato, sis: &str, trabajo: &Path) -> Vec<CrcJuego> {
    let mut out = vec![];
    match c.tipo {
        Tipo::Comprimido => {
            let entradas = c.entradas(e.siete_z).to_vec();
            for en in entradas {
                if !en.crc.is_empty() {
                    out.push(CrcJuego { crc: en.crc.clone(), salto: 0, entrada: Some(en.ruta.clone()) });
                }
                // Con cabecera hace falta el CRC sin ella, y para eso hay que sacar el fichero.
                let quizas = (SISTEMAS_SNES.contains(&sis) && en.tam % 1024 == 512) || en.tam % 1024 == 16;
                if quizas && en.tam <= MAX_CRC_COMPRIMIDO {
                    let Ok(t) = Temporal::nueva(trabajo) else { continue };
                    if descomprimir(e.siete_z, &c.ruta, &t.0, Some(&en.ruta)).is_ok() {
                        if let Some(f) = ficheros(&t.0).into_iter().next() {
                            let salto = cabecera(&f, sis);
                            if salto > 0 {
                                if let Ok(x) = motor::crc_fichero(&f, salto) {
                                    out.push(CrcJuego { crc: motor::hex(x), salto, entrada: Some(en.ruta.clone()) });
                                }
                            }
                        }
                    }
                }
            }
        }
        Tipo::Fichero if fs::metadata(&c.ruta).is_ok_and(|m| m.len() <= MAX_CRC) => crcs_de_fichero(&c.ruta, sis, &mut out),
        Tipo::Hoja => {
            for t in pistas(&c.ruta) {
                if fs::metadata(&t).is_ok_and(|m| m.len() <= MAX_CRC) {
                    if let Ok(x) = motor::crc_fichero(&t, 0) {
                        out.push(CrcJuego { crc: motor::hex(x), salto: 0, entrada: Some(t.to_string_lossy().into_owned()) });
                    }
                }
            }
        }
        // De un CHD no se puede sin extraerlo entero.
        _ => {}
    }
    out
}

enum Emparejado {
    Si { salto: u64, entrada: Option<String>, como: String },
    YaTraducido,
    No(String),
}

/// ¿Es tu juego la versión que pide la traducción? Por el CRC32 que lleva el parche (BPS y UPS) y los
/// de la ficha del catálogo. Si no se puede saber (un CHD o un juego muy grande), se intenta y el propio
/// parche lo comprueba al aplicarse.
fn emparejar(e: &Entorno, p: &Parche, c: &mut Candidato, sis: &str, crc_base: &[String], trabajo: &Path) -> Emparejado {
    let info = motor::inspeccionar(&p.fichero).unwrap_or_default();
    let mut buscados: Vec<String> = info.crc_origen.map(motor::hex).into_iter().collect();
    buscados.extend(crc_base.iter().map(|x| x.trim().to_uppercase()).filter(|x| x.len() == 8 && x.chars().all(|c| c.is_ascii_hexdigit())));
    if !buscados.is_empty() {
        let destino = info.crc_destino.map(motor::hex);
        let crcs = crcs_juego(e, c, sis, trabajo);
        for x in &crcs {
            if buscados.contains(&x.crc) {
                return Emparejado::Si { salto: x.salto, entrada: x.entrada.clone(), como: format!("CRC32 {}", x.crc) };
            }
            if destino.as_ref() == Some(&x.crc) {
                return Emparejado::YaTraducido;
            }
        }
        if info.crc_origen.is_some() || !crcs.is_empty() {
            return Emparejado::No(format!("Tu copia no es la versión que pide la traducción (CRC32 {}).", buscados[0]));
        }
    }
    Emparejado::Si { salto: 0, entrada: None, como: "elegido por ti".into() }
}

// ---------------------------------------------------------------- preparar el original

enum Origen {
    Rom { fichero: PathBuf, salto: u64, ext: String },
    Disco { hoja: PathBuf, pistas: Vec<PathBuf> },
    /// Un DVD (PS2, PSP) sacado de un CHD.
    Dvd { iso: PathBuf },
}

fn preparar(e: &Entorno, c: &mut Candidato, salto: u64, entrada: Option<&str>, trabajo: &Path) -> Result<Origen, String> {
    match c.tipo {
        Tipo::Comprimido => {
            let entradas = c.entradas(e.siete_z).to_vec();
            let dentro = trabajo.join("origen");
            let hoja = entradas.iter().find(|x| matches!(ext_de(Path::new(&x.ruta)).as_str(), "cue" | "gdi" | "chd" | "iso"));
            if hoja.is_some() {
                descomprimir(e.siete_z, &c.ruta, &dentro, None)?;
            } else if let Some(en) = entrada {
                descomprimir(e.siete_z, &c.ruta, &dentro, Some(en))?;
            } else {
                let mayor = entradas
                    .iter()
                    .filter(|x| !NO_JUEGO.contains(&ext_de(Path::new(&x.ruta)).as_str()))
                    .max_by_key(|x| x.tam)
                    .ok_or("El comprimido no tiene ningún juego.")?;
                descomprimir(e.siete_z, &c.ruta, &dentro, Some(&mayor.ruta))?;
            }
            let fs_ = ficheros(&dentro);
            let elegido = fs_
                .iter()
                .find(|f| matches!(ext_de(f).as_str(), "cue" | "gdi" | "chd"))
                .or_else(|| entrada.and_then(|en| fs_.iter().find(|f| nombre_de(f) == nombre_de(Path::new(en)))))
                .or_else(|| fs_.iter().max_by_key(|f| fs::metadata(f).map(|m| m.len()).unwrap_or(0)))
                .ok_or("El comprimido no tiene ningún juego.")?
                .clone();
            let mut sub = Candidato::de(&elegido);
            preparar(e, &mut sub, salto, None, trabajo)
        }
        Tipo::Chd => {
            let dir = trabajo.join("chd");
            fs::create_dir_all(&dir).map_err(|x| x.to_string())?;
            let (_, info) = herramienta(e.chdman, [OsStr::new("info"), OsStr::new("-i"), c.ruta.as_os_str()])?;
            let ok = if info.contains("Tag='DVD '") {
                let iso = dir.join("juego.iso");
                if herramienta(e.chdman, [OsStr::new("extractdvd"), "-i".as_ref(), c.ruta.as_os_str(), "-o".as_ref(), iso.as_os_str(), "-f".as_ref()])?.0 {
                    return Ok(Origen::Dvd { iso });
                }
                false
            } else if info.contains("Tag='CHGD'") {
                let gdi = dir.join("juego.gdi");
                herramienta(e.chdman, [OsStr::new("extractcd"), "-i".as_ref(), c.ruta.as_os_str(), "-o".as_ref(), gdi.as_os_str(), "-f".as_ref()])?.0
            } else {
                let cue = dir.join("juego.cue");
                let bin = dir.join("juego (Track %t).bin");
                herramienta(
                    e.chdman,
                    [OsStr::new("extractcd"), "-i".as_ref(), c.ruta.as_os_str(), "-o".as_ref(), cue.as_os_str(), "-ob".as_ref(), bin.as_os_str(), "-sb".as_ref(), "-f".as_ref()],
                )?
                .0
            };
            if !ok {
                return Err("chdman no pudo extraer el CHD.".into());
            }
            let hoja = ficheros(&dir).into_iter().find(|f| matches!(ext_de(f).as_str(), "cue" | "gdi")).ok_or("chdman no pudo extraer el CHD.")?;
            Ok(Origen::Disco { pistas: pistas(&hoja), hoja })
        }
        Tipo::Hoja => {
            let ps = pistas(&c.ruta);
            if let Some(f) = ps.iter().find(|t| !t.is_file()) {
                return Err(format!("Falta la pista {}.", nombre_de(f)));
            }
            Ok(Origen::Disco { pistas: ps, hoja: c.ruta.clone() })
        }
        Tipo::Fichero => {
            let ext = ext_de(&c.ruta);
            if let Some((_, n)) = SIN_PARCHEAR.iter().find(|(x, _)| *x == ext) {
                return Err(format!("Los {n} van comprimidos; conviértelo antes a .iso o .bin para poder parchearlo."));
            }
            Ok(Origen::Rom { fichero: c.ruta.clone(), salto, ext: format!(".{ext}") })
        }
    }
}

// ---------------------------------------------------------------- aplicar

fn xdelta(e: &Entorno, p: &Parche, origen: &Path, salida: &Path, salto: u64) -> Result<String, String> {
    let sin_cabecera = salida.with_extension("sincabecera");
    let src = if salto > 0 {
        let mut f = fs::File::open(origen).map_err(|x| x.to_string())?;
        std::io::Seek::seek(&mut f, std::io::SeekFrom::Start(salto)).map_err(|x| x.to_string())?;
        std::io::copy(&mut f, &mut fs::File::create(&sin_cabecera).map_err(|x| x.to_string())?).map_err(|x| x.to_string())?;
        sin_cabecera.as_path()
    } else {
        origen
    };
    let r = herramienta(e.xdelta, [OsStr::new("-d"), "-f".as_ref(), "-s".as_ref(), src.as_os_str(), p.fichero.as_os_str(), salida.as_os_str()]);
    let _ = fs::remove_file(&sin_cabecera);
    let (ok, texto) = r?;
    if ok {
        return Ok("xdelta aplicado y verificado por xdelta3.".into());
    }
    let _ = fs::remove_file(salida);
    let t = texto.to_lowercase();
    if t.contains("checksum mismatch") || t.contains("source file too short") || t.contains("invalid") {
        return Err("El juego no es la versión que pide el parche xdelta.".into());
    }
    Err(format!("xdelta3 falló: {}", texto.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("")))
}

fn aplicar_formato(e: &Entorno, p: &Parche, origen: &Path, salida: &Path, salto: u64) -> Result<String, String> {
    let r = if p.formato == Formato::Xdelta { xdelta(e, p, origen, salida, salto) } else { motor::aplicar(&p.fichero, origen, salida, salto, false) };
    // Sin la cabecera de NES el emulador no sabe arrancar el juego: se le vuelve a poner.
    if r.is_ok() && salto == 16 {
        let cab = fs::File::open(origen).and_then(|mut f| {
            let mut b = [0u8; 16];
            f.read_exact(&mut b).map(|_| b)
        });
        let cuerpo = fs::read(salida).map_err(|x| x.to_string())?;
        if let (Ok(cab), false) = (cab, cuerpo.starts_with(b"NES\x1a")) {
            let mut f = fs::File::create(salida).map_err(|x| x.to_string())?;
            f.write_all(&cab).and_then(|_| f.write_all(&cuerpo)).map_err(|x| x.to_string())?;
        }
    }
    r
}

/// Una ROM: si el parche no la reconoce con cabecera, se prueba sin ella (los BPS, UPS y xdelta lo saben).
fn aplicar_rom(e: &Entorno, p: &Parche, fichero: &Path, salida: &Path, salto: u64, sis: &str) -> Result<String, String> {
    let verificable = matches!(p.formato, Formato::Bps | Formato::Ups | Formato::Xdelta);
    // Los IPS de NES se hacen sobre la ROM con su cabecera.
    let salto = if salto == 16 && !verificable { 0 } else { salto };
    match aplicar_formato(e, p, fichero, salida, salto) {
        Err(m) if salto == 0 && verificable && m.contains("versión") && cabecera(fichero, sis) > 0 => {
            let cab = cabecera(fichero, sis);
            aplicar_formato(e, p, fichero, salida, cab).map(|m| format!("{m} (sin la cabecera de {cab} bytes)"))
        }
        Ok(mut m) => {
            if p.formato == Formato::Ips && salto == 0 && cabecera(fichero, sis) == 512 {
                m.push_str(" Aviso: tu ROM tiene cabecera de copiador; si el juego sale mal, quítasela y vuelve a traducir.");
            }
            Ok(m)
        }
        r => r,
    }
}

/// Quita etiquetas de traducción que ya tuviera el nombre ("[T-En]", "[T+Spa]"...).
fn sin_etiquetas(n: &str) -> String {
    let mut s = n.to_string();
    let mut desde = 0;
    while let Some(i) = s[desde..].find("[T").map(|i| desde + i) {
        match s[i..].find(']') {
            Some(f) if matches!(s.as_bytes().get(i + 2), Some(b'-' | b'+' | b' ')) => {
                let ini = s[..i].trim_end().len();
                s.replace_range(ini..i + f + 1, "");
                desde = ini;
            }
            _ => desde = i + 2,
        }
    }
    s.trim().to_string()
}

/// "<nombre> [T-Es]" y, si ya hay uno, "<nombre> [T-Es] (2)"...
fn nombre_salida(nombre: &str, dir: &Path) -> String {
    let limpio = sin_etiquetas(nombre);
    let ocupado = |base: &str| {
        let b = format!("{}.", base.to_lowercase());
        dir.join(base).exists() || fs::read_dir(dir).into_iter().flatten().flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().starts_with(&b))
    };
    let mut base = format!("{limpio} {ETIQUETA}");
    let mut n = 2;
    while ocupado(&base) {
        base = format!("{limpio} {ETIQUETA} ({n})");
        n += 1;
    }
    base
}

fn carpeta_destino(e: &Entorno, sis: &str, original: &Path) -> PathBuf {
    if e.salida.trim().is_empty() || e.salida == "junto" {
        original.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        Path::new(e.salida).join(sis)
    }
}

fn enlazar_o_copiar(de: &Path, a: &Path) -> Result<(), String> {
    if fs::hard_link(de, a).is_ok() {
        return Ok(());
    }
    fs::copy(de, a).map(|_| ()).map_err(|x| x.to_string())
}

/// Monta el disco traducido: las mismas pistas con la parcheada en su sitio, en un CHD.
fn montar_disco(e: &Entorno, hoja: &Path, ps: &[PathBuf], parcheada: &Path, original: &Path, trabajo: &Path, dest: &Path, base: &str) -> Result<PathBuf, String> {
    let montaje = trabajo.join("montaje");
    fs::create_dir_all(&montaje).map_err(|x| x.to_string())?;
    for t in ps {
        let d = montaje.join(t.file_name().unwrap_or_default());
        if t == original {
            mover(parcheada, &d)?;
        } else {
            enlazar_o_copiar(t, &d)?;
        }
    }
    let h = montaje.join(format!("juego.{}", ext_de(hoja)));
    fs::copy(hoja, &h).map_err(|x| x.to_string())?;
    let fin = dest.join(format!("{base}.chd"));
    if herramienta(e.chdman, [OsStr::new("createcd"), "-i".as_ref(), h.as_os_str(), "-o".as_ref(), fin.as_os_str(), "-f".as_ref()])?.0 {
        return Ok(fin);
    }
    let _ = fs::remove_file(&fin);
    Err("chdman no pudo crear el CHD del juego traducido.".into())
}

fn aplicar_uno(e: &Entorno, p: &Parche, c: &mut Candidato, sis: &str, salto: u64, entrada: Option<&str>, trabajo: &Path) -> Result<(PathBuf, String), String> {
    let origen = preparar(e, c, salto, entrada, trabajo)?;
    let dest = carpeta_destino(e, sis, &c.ruta);
    fs::create_dir_all(&dest).map_err(|x| format!("No se pudo crear {}: {x}", dest.display()))?;
    let base = nombre_salida(&c.nombre, &dest);
    match origen {
        Origen::Rom { fichero, salto, ext } => {
            let tmp = trabajo.join(format!("salida{ext}"));
            let msg = aplicar_rom(e, p, &fichero, &tmp, salto, sis)?;
            let fin = dest.join(format!("{base}{ext}"));
            mover(&tmp, &fin)?;
            Ok((fin, msg))
        }
        Origen::Dvd { iso } => {
            let tmp = trabajo.join("salida.iso");
            let msg = aplicar_formato(e, p, &iso, &tmp, 0)?;
            let fin = dest.join(format!("{base}.chd"));
            if !herramienta(e.chdman, [OsStr::new("createdvd"), "-i".as_ref(), tmp.as_os_str(), "-o".as_ref(), fin.as_os_str(), "-f".as_ref()])?.0 {
                let _ = fs::remove_file(&fin);
                return Err("chdman no pudo crear el CHD del juego traducido.".into());
            }
            Ok((fin, msg))
        }
        Origen::Disco { hoja, pistas: ps } => {
            // Primero la pista que el parche reconoce; si no lo dice, la primera y la más grande.
            let info = motor::inspeccionar(&p.fichero).unwrap_or_default();
            let mut orden: Vec<&PathBuf> = ps.iter().filter(|t| motor::comprobar(&info, t, 0) == Comprobacion::Vale).collect();
            orden.extend(ps.first());
            orden.extend(ps.iter().max_by_key(|t| fs::metadata(t).map(|m| m.len()).unwrap_or(0)));
            let mut vistos = vec![];
            let mut error = "El disco no tiene pistas.".to_string();
            for t in orden {
                if vistos.contains(&t) {
                    continue;
                }
                vistos.push(t);
                let tmp = trabajo.join(format!("pista_parcheada.{}", ext_de(t)));
                match aplicar_formato(e, p, t, &tmp, 0).and_then(|m| montar_disco(e, &hoja, &ps, &tmp, t, trabajo, &dest, &base).map(|f| (f, m))) {
                    Ok((f, m)) => return Ok((f, format!("{m} (pista {})", nombre_de(t)))),
                    Err(m) => {
                        error = m;
                        let _ = fs::remove_file(&tmp);
                    }
                }
            }
            Err(error)
        }
    }
}

// ---------------------------------------------------------------- lo ya hecho

fn clave_hecho(sis: &str, p: &Parche, juego: &Path) -> String {
    let len = fs::metadata(&p.original).map(|m| m.len()).unwrap_or(0);
    format!("{sis}|{}|{len}|{}|{}", nombre_de(&p.original), nombre_de(&p.fichero), juego.display())
}

fn hechos(e: &Entorno) -> HashMap<String, PathBuf> {
    fs::read_to_string(e.hechos)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(k, v)| (k.to_string(), PathBuf::from(v)))
        .collect()
}

fn apuntar_hecho(e: &Entorno, clave: &str, fin: &Path) {
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(e.hechos) {
        let _ = writeln!(f, "{clave}\t{}", fin.display());
    }
}

// ---------------------------------------------------------------- todo junto

/// Aplica la traducción de `archivo` (un parche o un comprimido con parches) a tu `juego`. Devuelve el
/// juego traducido o, si no se pudo, el motivo. Va dejando en `log` lo que hace.
pub fn traducir(e: &Entorno, sis: &str, archivo: &Path, juego: &Path, crc_base: &[String], log: &mut Vec<String>) -> Result<PathBuf, String> {
    fs::create_dir_all(e.temp).map_err(|x| x.to_string())?;
    let trabajo = Temporal::nueva(e.temp)?;
    let parches = parches_de(e, archivo, &trabajo.0)?;
    if parches.is_empty() {
        return Err(format!("{} no contiene ningún parche que se sepa aplicar.", nombre_de(archivo)));
    }
    if !juego.is_file() {
        return Err(format!("No encuentro {}.", juego.display()));
    }
    let mut c = Candidato::de(juego);
    let previos = hechos(e);
    let mut motivo = String::from("No se pudo aplicar la traducción.");
    for p in &parches {
        let nombre = nombre_de(&p.fichero);
        match emparejar(e, p, &mut c, sis, crc_base, &trabajo.0) {
            Emparejado::YaTraducido => {
                log.push(format!("= {nombre}: tu juego ya tiene aplicada esta traducción."));
                motivo = "Tu juego ya tiene aplicada esta traducción.".into();
            }
            Emparejado::No(m) => {
                log.push(format!("- {nombre}: {m}"));
                motivo = m;
            }
            Emparejado::Si { salto, entrada, como } => {
                let clave = clave_hecho(sis, p, juego);
                if let Some(f) = previos.get(&clave).filter(|f| f.is_file()) {
                    log.push(format!("= {nombre}: ya aplicado antes -> {}", nombre_de(f)));
                    return Ok(f.clone());
                }
                log.push(format!("+ {nombre} [{}] -> {} ({como})", p.formato.nombre(), nombre_de(juego)));
                let paso = Temporal::nueva(&trabajo.0)?;
                match aplicar_uno(e, p, &mut c, sis, salto, entrada.as_deref(), &paso.0) {
                    Ok((fin, m)) => {
                        log.push(format!("OK {m}"));
                        log.push(format!("-> {}", fin.display()));
                        apuntar_hecho(e, &clave, &fin);
                        return Ok(fin);
                    }
                    Err(m) => {
                        log.push(format!("ERROR {m}"));
                        motivo = m;
                    }
                }
            }
        }
    }
    Err(motivo)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::motor::pruebas::{bps_de, carpeta, ips_de, ruido, traducido, ups_de};

    fn herramientas() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("recursos").join("herramientas")
    }

    struct Prueba {
        dir: PathBuf,
        her: PathBuf,
        rutas: [PathBuf; 5],
        salida: String,
    }

    impl Prueba {
        fn nueva(nombre: &str) -> Prueba {
            let (dir, her) = (carpeta(nombre), herramientas());
            let rutas = [her.join("7z.exe"), her.join("chdman.exe"), her.join("xdelta3.exe"), dir.join("_temp"), dir.join("hechos.tsv")];
            Prueba { dir, her, rutas, salida: "junto".into() }
        }
        fn entorno(&self) -> Entorno<'_> {
            let [siete_z, chdman, xdelta, temp, hechos] = &self.rutas;
            Entorno { siete_z, chdman, xdelta, temp, hechos, salida: &self.salida }
        }
        fn fichero(&self, ruta: &str, b: &[u8]) -> PathBuf {
            // Con «\»: chdman no entiende las rutas con «/».
            let p = ruta.split('/').fold(self.dir.clone(), |a, x| a.join(x));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, b).unwrap();
            p
        }
        fn traducir(&self, sis: &str, parche: &Path, juego: &Path, crc: &[&str]) -> (Result<PathBuf, String>, Vec<String>) {
            let mut log = vec![];
            let crc: Vec<String> = crc.iter().map(|s| s.to_string()).collect();
            let r = traducir(&self.entorno(), sis, parche, juego, &crc, &mut log);
            (r, log)
        }
    }

    #[test]
    fn rom_suelta_bps() {
        let t = Prueba::nueva("parcheo-bps");
        let src = ruido(65536, 1);
        let tgt = traducido(&src);
        let juego = t.fichero("juegos/gba/Juego (Europe) [T-En].gba", &src);
        let parche = t.fichero("parche.bps", &bps_de(&src, &tgt));
        let (r, log) = t.traducir("gba", &parche, &juego, &[]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        assert_eq!(nombre_de(&fin), "Juego (Europe) [T-Es].gba");
        assert_eq!(fs::read(&fin).unwrap(), tgt);
        // Otra vez: devuelve el mismo, no hace otro.
        let (r2, _) = t.traducir("gba", &parche, &juego, &[]);
        assert_eq!(r2.unwrap(), fin);
        // Sobre el traducido: lo dice.
        let (r3, _) = t.traducir("gba", &parche, &fin, &[]);
        assert!(r3.unwrap_err().contains("ya tiene aplicada"));
        // Con otra versión: lo rechaza sin crear nada.
        let otra = t.fichero("juegos/gba/Otra.gba", &ruido(65536, 2));
        let (r4, _) = t.traducir("gba", &parche, &otra, &[]);
        assert!(r4.unwrap_err().contains("no es la versión"));
        assert!(!t.dir.join("juegos/gba/Otra [T-Es].gba").exists());
    }

    #[test]
    fn snes_con_cabecera_y_comprimidos() {
        let mut t = Prueba::nueva("parcheo-snes");
        let src = ruido(131072, 3);
        let tgt = traducido(&src);
        let mut con = vec![0u8; 512];
        con.extend_from_slice(&src);
        // La ROM con cabecera dentro de un .zip y el IPS (hecho sobre la ROM sin cabecera) en un .7z.
        let rom = t.fichero("tmp/Mi Juego (Spain).smc", &con);
        let zip = t.dir.join("juegos").join("Mi Juego (Spain).zip");
        fs::create_dir_all(zip.parent().unwrap()).unwrap();
        assert!(herramienta(&t.her.join("7z.exe"), [OsStr::new("a"), zip.as_os_str(), rom.as_os_str()]).unwrap().0);
        let ips = t.fichero("tmp/Mi Juego (Traducción).ips", &ips_de(&src, &tgt));
        let siete = t.dir.join("Mi Juego.7z");
        assert!(herramienta(&t.her.join("7z.exe"), [OsStr::new("a"), siete.as_os_str(), ips.as_os_str()]).unwrap().0);
        let crc = motor::hex(crc32fast::hash(&src));
        t.salida = t.dir.join("Traducidos").to_string_lossy().into_owned();
        let (r, log) = t.traducir("snes", &siete, &zip, &[&crc]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        assert_eq!(fin, t.dir.join("Traducidos").join("snes").join("Mi Juego (Spain) [T-Es].smc"));
        assert_eq!(fs::read(&fin).unwrap(), tgt);
        assert!(log.iter().any(|l| l.contains(&format!("CRC32 {crc}"))), "{log:?}");
        // El UPS hecho sobre la ROM sin cabecera también vale con la ROM suelta con cabecera.
        let suelta = t.fichero("juegos/Suelta.sfc", &con);
        let ups = t.fichero("p.ups", &ups_de(&src, &tgt));
        let (r, log) = t.traducir("snes", &ups, &suelta, &[]);
        assert_eq!(fs::read(r.unwrap_or_else(|e| panic!("{e}\n{log:?}"))).unwrap(), tgt);
    }

    #[test]
    fn nes_conserva_la_cabecera() {
        let t = Prueba::nueva("parcheo-nes");
        let src = ruido(40960, 4);
        let tgt = traducido(&src);
        let mut cab = b"NES\x1a\x02\x01".to_vec();
        cab.resize(16, 0);
        let juego = t.fichero("nes/Juego.nes", &[cab.clone(), src.clone()].concat());
        let parche = t.fichero("p.bps", &bps_de(&src, &tgt));
        let (r, log) = t.traducir("nes", &parche, &juego, &[]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        assert_eq!(fs::read(&fin).unwrap(), [cab, tgt].concat());
    }

    /// Un disco de dos pistas (datos y audio) con un xdelta para la primera: de .cue/.bin y de .chd.
    #[test]
    fn disco_xdelta_y_chd() {
        let t = Prueba::nueva("parcheo-disco");
        let e = t.entorno();
        let p1 = ruido(2352 * 300, 5);
        let p2 = ruido(2352 * 100, 6);
        let mut nueva = p1.clone();
        nueva[2352 * 20..2352 * 20 + 10].copy_from_slice(b"HOLA MUNDO");
        t.fichero("disco/Juego (Track 1).bin", &p1);
        t.fichero("disco/Juego (Track 2).bin", &p2);
        let cue = t.fichero(
            "disco/Juego.cue",
            b"FILE \"Juego (Track 1).bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\nFILE \"Juego (Track 2).bin\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 01 00:00:00\r\n",
        );
        let a = t.fichero("tmp/a.bin", &p1);
        let b = t.fichero("tmp/b.bin", &nueva);
        let xd = t.dir.join("parche.xdelta");
        assert!(herramienta(e.xdelta, [OsStr::new("-e"), "-f".as_ref(), "-s".as_ref(), a.as_os_str(), b.as_os_str(), xd.as_os_str()]).unwrap().0);

        let comprobar = |chd: &Path| {
            let d = t.dir.join("comprobar");
            let _ = fs::remove_dir_all(&d);
            fs::create_dir_all(&d).unwrap();
            let c = d.join("x.cue");
            assert!(herramienta(e.chdman, [OsStr::new("extractcd"), "-i".as_ref(), chd.as_os_str(), "-o".as_ref(), c.as_os_str(), "-ob".as_ref(), d.join("x (Track %t).bin").as_os_str(), "-sb".as_ref(), "-f".as_ref()]).unwrap().0);
            let ps = pistas(&c);
            assert_eq!(fs::read(&ps[0]).unwrap(), nueva);
            assert_eq!(fs::read(&ps[1]).unwrap(), p2);
        };

        let (r, log) = t.traducir("psx", &xd, &cue, &[]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        assert_eq!(nombre_de(&fin), "Juego [T-Es].chd");
        comprobar(&fin);

        // El mismo juego en CHD.
        let chd = t.dir.join("chd").join("Juego.chd");
        fs::create_dir_all(chd.parent().unwrap()).unwrap();
        let (ok, txt) = herramienta(e.chdman, [OsStr::new("createcd"), "-i".as_ref(), cue.as_os_str(), "-o".as_ref(), chd.as_os_str(), "-f".as_ref()]).unwrap();
        assert!(ok, "{txt}");
        let (r, log) = t.traducir("psx", &xd, &chd, &[]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        assert_eq!(fin, t.dir.join("chd").join("Juego [T-Es].chd"));
        comprobar(&fin);

        // Con un disco que no es: xdelta lo rechaza.
        t.fichero("otro/Otro (Track 1).bin", &ruido(2352 * 300, 9));
        let otro = t.fichero("otro/Otro.cue", b"FILE \"Otro (Track 1).bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n");
        let (r, _) = t.traducir("psx", &xd, &otro, &[]);
        assert!(r.unwrap_err().contains("no es la versión"));
    }

    #[test]
    fn nombres() {
        assert_eq!(sin_etiquetas("Juego (USA) [T-En by X] [!]"), "Juego (USA) [!]");
        assert_eq!(sin_etiquetas("Juego [T+Spa1.0]"), "Juego");
        assert_eq!(sin_etiquetas("Tetris [Tengen]"), "Tetris [Tengen]");
    }
}
