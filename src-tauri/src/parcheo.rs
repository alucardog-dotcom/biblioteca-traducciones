//! Aplicar una traducción a tu juego: lo que hacía Parchear.ps1 con -Juego, ya sin PowerShell.
//!
//! El parche puede venir suelto o dentro de un .zip/.7z/.rar, y el juego puede ser una ROM (suelta o
//! comprimida), un disco .cue/.gdi con sus pistas o un .chd. Se comprueba que el juego es la versión que
//! pide la traducción (por CRC32), se aplica con el motor (motor.rs) o con xdelta3.exe y el traducido se
//! deja con la etiqueta [T-Es] junto al original o en tu carpeta de traducidos. El original nunca se toca.

use crate::escaneo::{ext_de, pistas, pistas_tipo, ETIQUETA, NO_JUEGO};
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
        return Err(falta(&exe.file_name().unwrap_or_default().to_string_lossy()));
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

#[cfg(windows)]
fn falta(nombre: &str) -> String {
    format!("Falta {nombre} en las herramientas del programa.")
}
/// En Linux el AppImage puede no traer alguna herramienta: entonces se usa la del sistema, si está.
#[cfg(not(windows))]
fn falta(nombre: &str) -> String {
    let paquete = match nombre {
        "chdman" => "mame-tools o mame",
        "xdelta3" => "xdelta3",
        _ => "7zip o p7zip",
    };
    format!("Falta {nombre}: instálalo en el sistema (paquete {paquete}).")
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
    // `--`: lo que viene después es un nombre, aunque empiece por `-` o `@` (7z lo tomaría por un conmutador).
    let Ok((true, t)) = herramienta(siete_z, [OsStr::new("l"), OsStr::new("-slt"), OsStr::new("-ba"), OsStr::new("-sccUTF-8"), OsStr::new("--"), archivo.as_os_str()]) else {
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
    // `--`: el archivo y la entrada son nombres aunque empiecen por `-` (una entrada llamada «-spf» haría
    // que 7z extrajera con rutas absolutas, fuera de la carpeta temporal).
    args.push("--".into());
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

/// Muchos léame dicen el CRC32 de la ROM que pide el parche («CRC32: b78c09a2»). Sirve para comprobar tu
/// copia cuando la ficha del catálogo no lo trae: un IPS no lleva ninguno.
fn crcs_leeme(dir: &Path) -> Vec<String> {
    let mut out = vec![];
    for f in ficheros(dir) {
        let texto = matches!(ext_de(&f).as_str(), "txt" | "nfo" | "diz" | "md" | "");
        if !texto || fs::metadata(&f).map_or(true, |m| m.len() > 256 * 1024) {
            continue;
        }
        let Ok(b) = fs::read(&f) else { continue };
        for l in String::from_utf8_lossy(&b).to_lowercase().lines() {
            if let Some(c) = crc_de_linea(l).filter(|c| !out.contains(c)) {
                out.push(c);
            }
        }
    }
    out
}

/// «CRC32: B78C09A2», «CRC-32 = 0xb78c09a2», «CRC (sin cabecera): …» → "B78C09A2". El del juego ya
/// parcheado, que algunos léame también ponen, no vale.
fn crc_de_linea(l: &str) -> Option<String> {
    let i = l.find("crc")?;
    if ["parchead", "patched", "traducid", "translated", "resultado", "result", "output"].iter().any(|x| l.contains(x)) {
        return None;
    }
    let b = &l.as_bytes()[i + 3..];
    let mut j = 0;
    while j < b.len().min(60) {
        let suelto = j == 0 || !b[j - 1].is_ascii_alphanumeric() || (j >= 2 && &b[j - 2..j] == b"0x");
        if b[j].is_ascii_hexdigit() && suelto {
            let fin = j + b[j..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            if fin - j == 8 && !b.get(fin).is_some_and(|c| c.is_ascii_alphanumeric()) {
                return Some(String::from_utf8_lossy(&b[j..fin]).to_uppercase());
            }
            j = fin;
        } else {
            j += 1;
        }
    }
    None
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

/// Las sumas de `lista` con `cifras` cifras hexadecimales, en mayúsculas: 8 son de un CRC32 y 40 de un SHA-1.
fn sumas(lista: &[String], cifras: usize) -> Vec<String> {
    lista.iter().map(|x| x.trim().to_uppercase()).filter(|x| x.len() == cifras && x.chars().all(|c| c.is_ascii_hexdigit())).collect()
}

/// El fichero de tu juego, o la pista de tu disco, que tiene uno de esos SHA-1: (salto, pista, SHA-1). De
/// un comprimido o de un CHD no se puede sin sacarlos enteros.
fn por_sha1(c: &Candidato, sis: &str, buscados: &[String]) -> Option<(u64, Option<String>, String)> {
    let abarcable = |p: &Path| fs::metadata(p).is_ok_and(|m| m.len() <= MAX_CRC);
    let suma = |p: &Path, salto: u64| motor::sha1_fichero(p, salto).ok().map(|s| s.to_uppercase()).filter(|s| buscados.contains(s));
    match c.tipo {
        Tipo::Fichero if abarcable(&c.ruta) => {
            let cab = cabecera(&c.ruta, sis);
            let saltos = if cab > 0 { vec![0, cab] } else { vec![0] };
            saltos.into_iter().find_map(|salto| suma(&c.ruta, salto).map(|s| (salto, None, s)))
        }
        Tipo::Hoja => {
            // Primero las pistas de datos: el juego está en una de ellas.
            let mut ps = pistas_tipo(&c.ruta);
            ps.sort_by_key(|(_, datos)| !datos);
            ps.iter().filter(|(t, _)| abarcable(t)).find_map(|(t, _)| suma(t, 0).map(|s| (0, Some(t.to_string_lossy().into_owned()), s)))
        }
        _ => None,
    }
}

/// ¿Es tu juego la versión que pide la traducción? Por el CRC32 que lleva el parche (BPS y UPS) y por los
/// CRC32 y los SHA-1 de la ficha del catálogo (`crc_base` trae los dos, cada uno con sus cifras). En un
/// disco se mira pista a pista, y la que coincide es la que se parchea. Si no se puede saber (un CHD o un
/// juego muy grande), se intenta y el propio parche lo comprueba al aplicarse.
fn emparejar(e: &Entorno, p: &Parche, c: &mut Candidato, sis: &str, crc_base: &[String], trabajo: &Path) -> Emparejado {
    let info = motor::inspeccionar(&p.fichero).unwrap_or_default();
    let mut buscados: Vec<String> = info.crc_origen.map(motor::hex).into_iter().collect();
    buscados.extend(sumas(crc_base, 8));
    let mut otra_version = None;
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
        // Sin ningún CRC calculable (un CHD, un juego de más de 128 MB) se sigue adelante: BPS y UPS
        // comprueban el origen al aplicarse, y un IPS sin CRC es lo mismo que elegirlo a mano.
        if !crcs.is_empty() {
            otra_version = Some(format!("Tu copia no es la versión que pide la traducción (CRC32 {}).", buscados[0]));
        }
    }
    // Algunas fichas solo traen el SHA-1: también sirve para reconocer tu copia (y, en un disco, la pista).
    // Que no coincida no la descarta: eso solo lo hace el CRC32, como hasta ahora.
    let sha = sumas(crc_base, 40);
    if !sha.is_empty() {
        if let Some((salto, entrada, s)) = por_sha1(c, sis, &sha) {
            return Emparejado::Si { salto, entrada, como: format!("SHA-1 {s}") };
        }
    }
    match otra_version {
        Some(m) => Emparejado::No(m),
        None => Emparejado::Si { salto: 0, entrada: None, como: "elegido por ti".into() },
    }
}

// ---------------------------------------------------------------- preparar el original

enum Origen {
    Rom { fichero: PathBuf, salto: u64, ext: String },
    Disco { hoja: PathBuf, pistas: Vec<PathBuf> },
    /// Un DVD (PS2, PSP) sacado de un CHD.
    Dvd { iso: PathBuf },
}

/// Tu juego ya listo para parchear (sacado del comprimido o del CHD, si hacía falta). Se guarda para no
/// repetirlo con cada parche cuando el comprimido de la traducción trae varios que probar.
struct Preparado {
    salto: u64,
    entrada: Option<String>,
    origen: Origen,
    /// Donde está lo que se ha sacado: se borra al soltarlo.
    _carpeta: Temporal,
}

/// El juego listo para aplicarle un parche: el que ya estaba preparado, si era para lo mismo, o uno nuevo.
fn preparado<'a>(e: &Entorno, c: &mut Candidato, salto: u64, entrada: Option<&str>, trabajo: &Path, listo: &'a mut Option<Preparado>) -> Result<&'a Origen, String> {
    if !listo.as_ref().is_some_and(|x| x.salto == salto && x.entrada.as_deref() == entrada) {
        // Primero se suelta el anterior, para no tener dos copias del juego en el disco a la vez.
        *listo = None;
        let carpeta = Temporal::nueva(trabajo)?;
        let origen = preparar(e, c, salto, entrada, &carpeta.0)?;
        *listo = Some(Preparado { salto, entrada: entrada.map(str::to_string), origen, _carpeta: carpeta });
    }
    listo.as_ref().map(|x| &x.origen).ok_or_else(|| "No se pudo preparar el juego.".to_string())
}

fn preparar(e: &Entorno, c: &mut Candidato, salto: u64, entrada: Option<&str>, trabajo: &Path) -> Result<Origen, String> {
    preparar_nivel(e, c, salto, entrada, trabajo, 0)
}

/// `nivel`: cuántos comprimidos se han abierto ya, uno dentro de otro. Con tope, y sin volver a elegir el
/// comprimido que se acaba de abrir, para que un zip dentro de un zip no dé vueltas sin fin.
fn preparar_nivel(e: &Entorno, c: &mut Candidato, salto: u64, entrada: Option<&str>, trabajo: &Path, nivel: u8) -> Result<Origen, String> {
    match c.tipo {
        Tipo::Comprimido => {
            if nivel >= 3 {
                return Err("El juego está comprimido demasiadas veces, uno dentro de otro.".into());
            }
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
            let fs_: Vec<PathBuf> = ficheros(&dentro).into_iter().filter(|f| f.as_path() != c.ruta.as_path()).collect();
            let elegido = fs_
                .iter()
                .find(|f| matches!(ext_de(f).as_str(), "cue" | "gdi" | "chd"))
                .or_else(|| entrada.and_then(|en| fs_.iter().find(|f| nombre_de(f) == nombre_de(Path::new(en)))))
                .or_else(|| fs_.iter().max_by_key(|f| fs::metadata(f).map(|m| m.len()).unwrap_or(0)))
                .ok_or("El comprimido no tiene ningún juego.")?
                .clone();
            let mut sub = Candidato::de(&elegido);
            preparar_nivel(e, &mut sub, salto, None, trabajo, nivel + 1)
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

/// Con .bin, el emulador no sabe que es de Game Gear, Master System, SG-1000 o 32X: lo abre como si fuera
/// de otra consola y la pantalla se queda en negro. El traducido lleva la extensión de su sistema.
fn ext_final(sis: &str, ext: &str) -> String {
    let propia = match sis {
        "gamegear" => ".gg",
        "mastersystem" => ".sms",
        "sg1000" => ".sg",
        "sega32x" => ".32x",
        _ => "",
    };
    if ext.eq_ignore_ascii_case(".bin") && !propia.is_empty() { propia.into() } else { ext.into() }
}

/// Un traducido de antes que se quedó con .bin: se renombra con la extensión de su sistema. Devuelve la
/// ruta nueva si lo ha renombrado. Solo toca los traducidos (con la etiqueta), nunca un original.
pub fn corregir_extension(ruta: &Path, sis: &str) -> Option<PathBuf> {
    if !nombre_de(ruta).contains(ETIQUETA) || !ruta.is_file() {
        return None;
    }
    let ext = format!(".{}", ext_de(ruta));
    let buena = ext_final(sis, &ext);
    if buena == ext {
        return None;
    }
    let nueva = ruta.with_extension(&buena[1..]);
    (!nueva.exists() && fs::rename(ruta, &nueva).is_ok()).then_some(nueva)
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

/// Las pistas de un disco con las que probar el parche, por orden. El juego no siempre va en la primera ni
/// en la más grande: en PC Engine CD la primera es de audio y los datos van en la segunda.
///  1. La que ya se reconoció por el CRC32 o el SHA-1 de la ficha (`entrada`).
///  2. Las que el propio parche reconoce (BPS y UPS por su CRC32, PPF por su bloque).
///  3. Las de datos según el .cue o el .gdi; si la hoja no lo dice, la primera y la más grande.
///  4. Con un xdelta, todas las demás: xdelta3 comprueba el resultado, así que probar no estropea nada.
/// Las que el parche descarta no se prueban. Y con un parche que no comprueba nada (un IPS, un PPF sin
/// bloque) no se pasa de las de datos: se dejaría aplicar a una pista de audio y la estropearía.
fn orden_pistas<'a>(p: &Parche, hoja: &Path, ps: &'a [PathBuf], entrada: Option<&str>) -> Vec<&'a PathBuf> {
    let info = motor::inspeccionar(&p.fichero).unwrap_or_default();
    let estado: Vec<Comprobacion> = ps.iter().map(|t| motor::comprobar(&info, t, 0)).collect();
    let tipos = pistas_tipo(hoja);
    let de_datos = |t: &PathBuf| tipos.iter().any(|(x, datos)| *datos && x == t);
    let es_entrada = |t: &PathBuf| entrada.is_some_and(|en| t.as_path() == Path::new(en) || nombre_de(t) == nombre_de(Path::new(en)));
    let todas = 0..ps.len();
    let mut orden: Vec<usize> = todas.clone().filter(|&i| es_entrada(&ps[i])).collect();
    orden.extend(todas.clone().filter(|&i| estado[i] == Comprobacion::Vale));
    orden.extend(todas.clone().filter(|&i| estado[i] == Comprobacion::YaParcheado));
    let datos: Vec<usize> = todas.clone().filter(|&i| de_datos(&ps[i])).collect();
    if datos.is_empty() {
        orden.extend(todas.clone().next());
        orden.extend(todas.clone().max_by_key(|&i| fs::metadata(&ps[i]).map(|m| m.len()).unwrap_or(0)));
    } else {
        orden.extend(datos);
    }
    if p.formato == Formato::Xdelta {
        orden.extend(todas);
    }
    let mut vistas = vec![];
    orden.retain(|&i| {
        let nueva = estado[i] != Comprobacion::NoCoincide && !vistas.contains(&i);
        vistas.push(i);
        nueva
    });
    orden.into_iter().map(|i| &ps[i]).collect()
}

fn aplicar_uno(e: &Entorno, p: &Parche, c: &Candidato, sis: &str, origen: &Origen, entrada: Option<&str>, trabajo: &Path) -> Result<(PathBuf, String), String> {
    let dest = carpeta_destino(e, sis, &c.ruta);
    fs::create_dir_all(&dest).map_err(|x| format!("No se pudo crear {}: {x}", dest.display()))?;
    let base = nombre_salida(&c.nombre, &dest);
    match origen {
        Origen::Rom { fichero, salto, ext } => {
            let tmp = trabajo.join(format!("salida{ext}"));
            let msg = aplicar_rom(e, p, fichero, &tmp, *salto, sis)?;
            let fin = dest.join(format!("{base}{}", ext_final(sis, ext)));
            mover(&tmp, &fin)?;
            Ok((fin, msg))
        }
        Origen::Dvd { iso } => {
            let tmp = trabajo.join("salida.iso");
            let msg = aplicar_formato(e, p, iso, &tmp, 0)?;
            let fin = dest.join(format!("{base}.chd"));
            if !herramienta(e.chdman, [OsStr::new("createdvd"), "-i".as_ref(), tmp.as_os_str(), "-o".as_ref(), fin.as_os_str(), "-f".as_ref()])?.0 {
                let _ = fs::remove_file(&fin);
                return Err("chdman no pudo crear el CHD del juego traducido.".into());
            }
            Ok((fin, msg))
        }
        Origen::Disco { hoja, pistas: ps } => {
            // Se queda con el motivo de la primera pista que falla, que es la que más papeletas tenía.
            let mut error: Option<String> = None;
            for t in orden_pistas(p, hoja, ps, entrada) {
                let tmp = trabajo.join(format!("pista_parcheada.{}", ext_de(t)));
                match aplicar_formato(e, p, t, &tmp, 0) {
                    // Con la pista ya parcheada, si el disco no se puede montar no hay otra que probar.
                    Ok(m) => return montar_disco(e, hoja, ps, &tmp, t, trabajo, &dest, &base).map(|f| (f, format!("{m} (pista {})", nombre_de(t)))),
                    Err(m) => {
                        let _ = fs::remove_file(&tmp);
                        // Esa era la pista, y ya está traducida.
                        if m.contains("ya tiene aplicado") {
                            return Err(m);
                        }
                        error.get_or_insert(m);
                    }
                }
            }
            Err(error.unwrap_or_else(|| if ps.is_empty() { "El disco no tiene pistas." } else { "El disco no es la versión que pide el parche: no coincide ninguna de sus pistas." }.to_string()))
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
    let mut crc_base = crc_base.to_vec();
    for x in crcs_leeme(&trabajo.0.join("parches")) {
        if !crc_base.iter().any(|y| y.trim().eq_ignore_ascii_case(&x)) {
            log.push(format!("· El léame pide la ROM con CRC32 {x}."));
            crc_base.push(x);
        }
    }
    let mut motivo = String::from("No se pudo aplicar la traducción.");
    // Un comprimido puede traer varios parches: uno por versión del juego, o uno por cada forma de volcar
    // el mismo disco (pistas sueltas, imagen en un solo fichero, ISO + WAV…). Se prueban todos, por orden,
    // hasta dar con el que vale para tu copia, sin sacar el juego del comprimido o del CHD más de una vez.
    let mut listo: Option<Preparado> = None;
    let dentro = trabajo.0.join("parches");
    for p in &parches {
        // Con su carpeta, que si no los cinco «Juego_ES.xdelta» de un mismo comprimido no se distinguen.
        let nombre = match p.fichero.strip_prefix(&dentro) {
            Ok(r) if parches.len() > 1 => r.display().to_string(),
            _ => nombre_de(&p.fichero),
        };
        match emparejar(e, p, &mut c, sis, &crc_base, &trabajo.0) {
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
                    let f = match corregir_extension(f, sis) {
                        Some(nueva) => {
                            apuntar_hecho(e, &clave, &nueva);
                            nueva
                        }
                        None => f.clone(),
                    };
                    log.push(format!("= {nombre}: ya aplicado antes -> {}", nombre_de(&f)));
                    return Ok(f);
                }
                log.push(format!("+ {nombre} [{}] -> {} ({como})", p.formato.nombre(), nombre_de(juego)));
                if como == "elegido por ti" && p.formato == Formato::Ips {
                    log.push("  Aviso: ni la ficha ni el léame dicen qué versión pide, así que no se ha podido comprobar tu copia. Si el juego no arranca, es otra versión.".into());
                }
                let paso = Temporal::nueva(&trabajo.0)?;
                let hecho = preparado(e, &mut c, salto, entrada.as_deref(), &trabajo.0, &mut listo).and_then(|origen| aplicar_uno(e, p, &c, sis, origen, entrada.as_deref(), &paso.0));
                match hecho {
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

    /// Una ruta del registro con el separador del sistema (`\` en Windows, `/` en Linux).
    fn sep(s: &str) -> String {
        s.replace('\\', std::path::MAIN_SEPARATOR_STR)
    }

    struct Prueba {
        dir: PathBuf,
        rutas: [PathBuf; 5],
        salida: String,
    }

    /// Las herramientas, por su sitio en `Prueba::rutas`.
    const SIETE_Z: usize = 0;
    const CHDMAN: usize = 1;
    const XDELTA: usize = 2;
    const TODAS: &[usize] = &[SIETE_Z, CHDMAN, XDELTA];

    impl Prueba {
        fn nueva(nombre: &str) -> Prueba {
            let dir = carpeta(nombre);
            // Las mismas que usa el programa: en Windows, las .exe de recursos/herramientas; en Linux, las de
            // recursos/herramientas-linux (si se han preparado) o las del sistema.
            #[cfg(windows)]
            let her = herramientas();
            #[cfg(not(windows))]
            let her = herramientas().with_file_name("herramientas-linux");
            let [siete_z, chdman, xdelta] = crate::rutas::herramientas_en(&her);
            let rutas = [siete_z, chdman, xdelta, dir.join("_temp"), dir.join("hechos.tsv")];
            Prueba { dir, rutas, salida: "junto".into() }
        }
        /// La prueba, si están las herramientas que necesita. En Linux, si falta alguna (chdman suele faltar),
        /// se salta con un aviso en lugar de fallar; en Windows las lleva el programa y falla como siempre.
        fn con(nombre: &str, necesita: &[usize]) -> Option<Prueba> {
            let t = Prueba::nueva(nombre);
            let faltan: Vec<String> = necesita.iter().map(|&i| &t.rutas[i]).filter(|p| !p.is_file()).map(|p| p.display().to_string()).collect();
            if faltan.is_empty() || cfg!(windows) {
                return Some(t);
            }
            eprintln!("AVISO: se salta la prueba {nombre}: falta {}", faltan.join(", "));
            None
        }
        fn her(&self, i: usize) -> &Path {
            &self.rutas[i]
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
    fn crc_del_leeme() {
        assert_eq!(crc_de_linea("crc32: b78c09a2"), Some("B78C09A2".into()));
        assert_eq!(crc_de_linea("crc-32 = 0xb78c09a2"), Some("B78C09A2".into()));
        assert_eq!(crc_de_linea("crc32 (sin cabecera): 1234abcd"), Some("1234ABCD".into()));
        assert_eq!(crc_de_linea("crc32 del juego parcheado: 1234abcd"), None);
        assert_eq!(crc_de_linea("md5: be07098de1de3dc2512b6aa93f23ae3a"), None);
        assert_eq!(crc_de_linea("crc32: be07098de1de3dc2"), None);
    }

    #[test]
    fn ips_con_leeme_comprueba_la_version() {
        let Some(t) = Prueba::con("parcheo-leeme", &[SIETE_Z]) else { return };
        let src = ruido(65536, 5);
        let tgt = traducido(&src);
        let crc = motor::hex(crc32fast::hash(&src));
        let ips = t.fichero("tmp/juego.ips", &ips_de(&src, &tgt));
        let leeme = t.fichero("tmp/juego_readme.txt", format!("Instrucciones de Parcheo\r\nCRC32: {}\r\n", crc.to_lowercase()).as_bytes());
        let zip = t.dir.join("juego.zip");
        assert!(herramienta(t.her(SIETE_Z), [OsStr::new("a"), zip.as_os_str(), ips.as_os_str(), leeme.as_os_str()]).unwrap().0);
        // Otra versión del juego: el IPS no lo sabría, pero el léame sí.
        let otra = t.fichero("juegos/Juego (Europe).nes", &ruido(65536, 6));
        let (r, log) = t.traducir("nes", &zip, &otra, &[]);
        assert!(r.unwrap_err().contains(&format!("no es la versión que pide la traducción (CRC32 {crc})")), "{log:?}");
        assert!(!t.dir.join("juegos/Juego (Europe) [T-Es].nes").exists());
        // La buena: se aplica.
        let buena = t.fichero("juegos/Juego (USA).nes", &src);
        let (r, log) = t.traducir("nes", &zip, &buena, &[]);
        assert_eq!(fs::read(r.unwrap_or_else(|e| panic!("{e}\n{log:?}"))).unwrap(), tgt);
    }

    #[test]
    fn game_gear_en_bin_sale_en_gg() {
        let t = Prueba::nueva("parcheo-gg");
        let src = ruido(65536, 4);
        let tgt = traducido(&src);
        let juego = t.fichero("juegos/GG Aleste (1991)(Compile)(JP)(en).bin", &src);
        let parche = t.fichero("parche.bps", &bps_de(&src, &tgt));
        let (r, log) = t.traducir("gamegear", &parche, &juego, &[]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        assert_eq!(nombre_de(&fin), "GG Aleste (1991)(Compile)(JP)(en) [T-Es].gg");
        // Un traducido de antes con .bin se renombra al volver a traducir, y queda apuntado.
        let viejo = fin.with_extension("bin");
        fs::rename(&fin, &viejo).unwrap();
        apuntar_hecho(&t.entorno(), &clave_hecho("gamegear", &parches_de(&t.entorno(), &parche, &t.dir).unwrap()[0], &juego), &viejo);
        let (r2, _) = t.traducir("gamegear", &parche, &juego, &[]);
        assert_eq!(r2.unwrap(), fin);
        assert!(fin.is_file() && !viejo.exists());
        // Los originales y los de Mega Drive no se tocan.
        assert_eq!(corregir_extension(&juego, "gamegear"), None);
        let md = t.fichero("juegos/Otro [T-Es].bin", &src);
        assert_eq!(corregir_extension(&md, "megadrive"), None);
    }

    #[test]
    fn snes_con_cabecera_y_comprimidos() {
        let Some(mut t) = Prueba::con("parcheo-snes", &[SIETE_Z]) else { return };
        let src = ruido(131072, 3);
        let tgt = traducido(&src);
        let mut con = vec![0u8; 512];
        con.extend_from_slice(&src);
        // La ROM con cabecera dentro de un .zip y el IPS (hecho sobre la ROM sin cabecera) en un .7z.
        let rom = t.fichero("tmp/Mi Juego (Spain).smc", &con);
        let zip = t.dir.join("juegos").join("Mi Juego (Spain).zip");
        fs::create_dir_all(zip.parent().unwrap()).unwrap();
        assert!(herramienta(t.her(SIETE_Z), [OsStr::new("a"), zip.as_os_str(), rom.as_os_str()]).unwrap().0);
        let ips = t.fichero("tmp/Mi Juego (Traducción).ips", &ips_de(&src, &tgt));
        let siete = t.dir.join("Mi Juego.7z");
        assert!(herramienta(t.her(SIETE_Z), [OsStr::new("a"), siete.as_os_str(), ips.as_os_str()]).unwrap().0);
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
        let Some(t) = Prueba::con("parcheo-disco", TODAS) else { return };
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

    // ---- discos con el juego en otra pista que la primera (PC Engine CD) y comprimidos con varios parches

    const SECTOR: usize = 2352;
    /// Como los de PC Engine CD: la pista 1 es de audio, el juego va en la 2 (con su pregap) y la 3, de
    /// audio, es la más grande.
    const CUE_PISTAS: &str = "FILE \"Juego (Track 1).bin\" BINARY\r\n  TRACK 01 AUDIO\r\n    INDEX 01 00:00:00\r\nFILE \"Juego (Track 2).bin\" BINARY\r\n  TRACK 02 MODE1/2352\r\n    INDEX 00 00:00:00\r\n    INDEX 01 00:03:00\r\nFILE \"Juego (Track 3).bin\" BINARY\r\n  TRACK 03 AUDIO\r\n    INDEX 00 00:00:00\r\n    INDEX 01 00:02:00\r\n";
    /// El mismo disco en un solo .bin: 200 sectores de la pista 1, 400 de la 2 y 600 de la 3.
    const CUE_IMAGEN: &str = "FILE \"Juego.bin\" BINARY\r\n  TRACK 01 AUDIO\r\n    INDEX 01 00:00:00\r\n  TRACK 02 MODE1/2352\r\n    INDEX 00 00:02:50\r\n    INDEX 01 00:05:50\r\n  TRACK 03 AUDIO\r\n    INDEX 00 00:08:00\r\n    INDEX 01 00:10:00\r\n";

    /// Y en «ISO + WAV», como los volcados antiguos.
    const CUE_ISO_WAV: &str = "FILE \"Juego 01.wav\" WAVE\r\n  TRACK 01 AUDIO\r\n    INDEX 01 00:00:00\r\nFILE \"Juego 02.iso\" BINARY\r\n  TRACK 02 MODE1/2048\r\n    INDEX 01 00:00:00\r\nFILE \"Juego 03.wav\" WAVE\r\n  TRACK 03 AUDIO\r\n    INDEX 01 00:00:00\r\n";

    /// Un .wav de CD (44,1 kHz, 16 bits, estéreo) con ese sonido.
    fn wav_de(pcm: &[u8]) -> Vec<u8> {
        let mut w = b"RIFF".to_vec();
        w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        // Tamaño del bloque, PCM, canales, muestras por segundo, bytes por segundo, bytes por muestra y bits.
        for (n, bytes) in [(16u32, 4), (1, 2), (2, 2), (44100, 4), (176400, 4), (4, 2), (16, 2)] {
            w.extend_from_slice(&n.to_le_bytes()[..bytes]);
        }
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(pcm);
        w
    }

    fn pistas_inventadas(semilla: u32) -> [Vec<u8>; 3] {
        [ruido(SECTOR * 200, semilla), ruido(SECTOR * 400, semilla + 1), ruido(SECTOR * 600, semilla + 2)]
    }

    /// Escribe el disco de pistas sueltas en `dir` y devuelve su .cue.
    fn disco_de_pistas(t: &Prueba, dir: &str, ps: &[Vec<u8>]) -> PathBuf {
        for (i, p) in ps.iter().enumerate() {
            t.fichero(&format!("{dir}/Juego (Track {}).bin", i + 1), p);
        }
        t.fichero(&format!("{dir}/Juego.cue"), CUE_PISTAS.as_bytes())
    }

    /// La «traducción» de una pista: un texto en un sector, sin cambiarle el tamaño.
    fn con_texto(p: &[u8], sector: usize) -> Vec<u8> {
        let mut n = p.to_vec();
        n[SECTOR * sector..SECTOR * sector + 10].copy_from_slice(b"HOLA MUNDO");
        n
    }

    /// Un xdelta que lleva de `de` a `a`, hecho con el xdelta3 del programa.
    fn xdelta_de(t: &Prueba, ruta: &str, de: &[u8], a: &[u8]) -> PathBuf {
        let (o, d) = (t.fichero("tmp/xdelta-origen.bin", de), t.fichero("tmp/xdelta-destino.bin", a));
        let xd = t.fichero(ruta, b"");
        let (ok, txt) = herramienta(t.her(XDELTA), [OsStr::new("-e"), "-f".as_ref(), "-s".as_ref(), o.as_os_str(), d.as_os_str(), xd.as_os_str()]).unwrap();
        assert!(ok, "{txt}");
        xd
    }

    fn chd_de(t: &Prueba, cue: &Path, ruta: &str) -> PathBuf {
        let chd = t.fichero(ruta, b"");
        let (ok, txt) = herramienta(t.her(CHDMAN), [OsStr::new("createcd"), "-i".as_ref(), cue.as_os_str(), "-o".as_ref(), chd.as_os_str(), "-f".as_ref()]).unwrap();
        assert!(ok, "{txt}");
        chd
    }

    /// Lo que lleva un .chd: sus pistas, una por fichero (como las saca el programa), o el disco entero.
    fn contenido_chd(t: &Prueba, chd: &Path, por_pistas: bool) -> Vec<Vec<u8>> {
        let d = t.dir.join("comprobar");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let (c, bin) = (d.join("x.cue"), d.join(if por_pistas { "x (Track %t).bin" } else { "x.bin" }));
        let mut args = vec![OsStr::new("extractcd"), "-i".as_ref(), chd.as_os_str(), "-o".as_ref(), c.as_os_str(), "-ob".as_ref(), bin.as_os_str(), "-f".as_ref()];
        if por_pistas {
            args.push("-sb".as_ref());
        }
        let (ok, txt) = herramienta(t.her(CHDMAN), args).unwrap();
        assert!(ok, "{txt}");
        pistas(&c).iter().map(|p| fs::read(p).unwrap()).collect()
    }

    /// Compara lo que lleva un disco con lo esperado. Si falla dice dónde, sin volcar megas de bytes.
    #[track_caller]
    fn iguales(hay: &[Vec<u8>], esperado: &[Vec<u8>]) {
        assert_eq!(hay.iter().map(Vec::len).collect::<Vec<_>>(), esperado.iter().map(Vec::len).collect::<Vec<_>>(), "pistas de otro tamaño");
        for (i, (a, b)) in hay.iter().zip(esperado).enumerate() {
            if let Some(en) = a.iter().zip(b).position(|(x, y)| x != y) {
                panic!("la pista {} cambia en el byte {en} (sector {})", i + 1, en / SECTOR);
            }
        }
    }

    /// El fallo de Rondo of Blood: el xdelta es para la pista 2, que no es ni la primera ni la más grande.
    #[test]
    fn disco_con_el_juego_en_la_pista_2() {
        let Some(t) = Prueba::con("parcheo-pista2", TODAS) else { return };
        let ps = pistas_inventadas(20);
        let nueva = con_texto(&ps[1], 250);
        let xd = xdelta_de(&t, "parche.xdelta", &ps[1], &nueva);
        let crc = motor::hex(crc32fast::hash(&ps[1]));
        let esperado = [ps[0].clone(), nueva.clone(), ps[2].clone()];

        // La ficha trae dos CRC32 (dos volcados del mismo juego): vale el segundo igual que el primero.
        let cue = disco_de_pistas(&t, "a", &ps);
        let (r, log) = t.traducir("pcenginecd", &xd, &cue, &["0BADC0DE", &crc]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        assert_eq!(nombre_de(&fin), "Juego [T-Es].chd");
        iguales(&contenido_chd(&t, &fin, true), &esperado);
        assert!(log.iter().any(|l| l.contains(&format!("CRC32 {crc}"))), "{log:?}");
        assert!(log.iter().any(|l| l.contains("(pista Juego (Track 2).bin)")), "{log:?}");
        let cue = disco_de_pistas(&t, "b", &ps);
        let (r, log) = t.traducir("pcenginecd", &xd, &cue, &[&crc.to_lowercase(), "0BADC0DE"]);
        iguales(&contenido_chd(&t, &r.unwrap_or_else(|e| panic!("{e}\n{log:?}")), true), &esperado);

        // Sin CRC32 en la ficha: se prueban las pistas, la de datos primero.
        let cue = disco_de_pistas(&t, "c", &ps);
        let (r, log) = t.traducir("pcenginecd", &xd, &cue, &[]);
        iguales(&contenido_chd(&t, &r.unwrap_or_else(|e| panic!("{e}\n{log:?}")), true), &esperado);

        // Aunque el .cue no diga cuál es la de datos (aquí, todas de audio): se prueban todas.
        let cue = disco_de_pistas(&t, "d", &ps);
        fs::write(&cue, CUE_PISTAS.replace("MODE1/2352", "AUDIO")).unwrap();
        let (r, log) = t.traducir("pcenginecd", &xd, &cue, &[]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        assert!(log.iter().any(|l| l.contains("(pista Juego (Track 2).bin)")), "{log:?}");
        assert!(fin.is_file());

        // El mismo disco en .chd: de ahí no sale ningún CRC32, se saca y se prueba.
        let chd = chd_de(&t, &disco_de_pistas(&t, "e", &ps), "chd/Juego.chd");
        let (r, log) = t.traducir("pcenginecd", &xd, &chd, &["0BADC0DE", &crc]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        assert_eq!(fin, t.dir.join("chd").join("Juego [T-Es].chd"));
        iguales(&contenido_chd(&t, &fin, true), &esperado);

        // Otro volcado, que no está entre los de la ficha: se rechaza sin hacer nada.
        let cue = disco_de_pistas(&t, "f", &pistas_inventadas(30));
        let (r, _) = t.traducir("pcenginecd", &xd, &cue, &["0BADC0DE", &crc]);
        assert!(r.unwrap_err().contains("no es la versión que pide la traducción"));
        assert!(!t.dir.join("f").join("Juego [T-Es].chd").exists());
        // Y sin CRC32 con el que rechazarlo antes, lo rechaza xdelta3 en todas las pistas.
        let (r, _) = t.traducir("pcenginecd", &xd, &cue, &[]);
        assert!(r.unwrap_err().contains("no es la versión"));
        assert!(!t.dir.join("f").join("Juego [T-Es].chd").exists());
    }

    /// Un IPS no comprueba nada: va a la pista de datos, nunca a una de audio.
    #[test]
    fn ips_en_un_disco_no_toca_el_audio() {
        let Some(t) = Prueba::con("parcheo-ips-disco", TODAS) else { return };
        let ps = pistas_inventadas(40);
        let nueva = con_texto(&ps[1], 250);
        let ips = t.fichero("parche.ips", &ips_de(&ps[1], &nueva));
        let cue = disco_de_pistas(&t, "a", &ps);
        let (r, log) = t.traducir("pcenginecd", &ips, &cue, &[]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        iguales(&contenido_chd(&t, &fin, true), &[ps[0].clone(), nueva, ps[2].clone()]);
    }

    /// Con dos pistas de datos y un parche que no comprueba nada, la ficha dice cuál: por su CRC32 o,
    /// si solo trae el SHA-1, por él.
    #[test]
    fn la_ficha_dice_la_pista() {
        let Some(t) = Prueba::con("parcheo-sha1", TODAS) else { return };
        let ps = pistas_inventadas(50);
        let nueva = con_texto(&ps[2], 300);
        let ips = t.fichero("parche.ips", &ips_de(&ps[2], &nueva));
        let dos_de_datos = CUE_PISTAS.replace("TRACK 03 AUDIO", "TRACK 03 MODE1/2352");
        let esperado = [ps[0].clone(), ps[1].clone(), nueva.clone()];
        let sha = |b: &[u8]| sha1_smol::Sha1::from(b).digest().to_string();
        let otro = "0123456789abcdef0123456789abcdef01234567";

        let cue = disco_de_pistas(&t, "a", &ps);
        fs::write(&cue, &dos_de_datos).unwrap();
        let (r, log) = t.traducir("pcenginecd", &ips, &cue, &[otro, &sha(&ps[2]).to_uppercase()]);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        iguales(&contenido_chd(&t, &fin, true), &esperado);
        assert!(log.iter().any(|l| l.contains("(SHA-1 ")), "{log:?}");

        let cue = disco_de_pistas(&t, "b", &ps);
        fs::write(&cue, &dos_de_datos).unwrap();
        let (r, log) = t.traducir("pcenginecd", &ips, &cue, &[&motor::hex(crc32fast::hash(&ps[2]))]);
        iguales(&contenido_chd(&t, &r.unwrap_or_else(|e| panic!("{e}\n{log:?}")), true), &esperado);

        // Un SHA-1 que no coincide no rechaza la copia (eso solo lo hace el CRC32): va a la primera de datos.
        let cue = disco_de_pistas(&t, "c", &ps);
        fs::write(&cue, &dos_de_datos).unwrap();
        let (r, log) = t.traducir("pcenginecd", &ips, &cue, &[otro]);
        assert!(r.is_ok() && log.iter().any(|l| l.contains("elegido por ti")), "{log:?}");

        // También con una ROM suelta: el SHA-1 de la ficha (aquí, el de la ROM sin su cabecera) la reconoce.
        let rom = ruido(40960, 7);
        let mut con = b"NES\x1a\x02\x01".to_vec();
        con.resize(16, 0);
        con.extend_from_slice(&rom);
        let juego = t.fichero("nes/Juego.nes", &con);
        let ips = t.fichero("rom.ips", &ips_de(&con, &traducido(&con)));
        let (r, log) = t.traducir("nes", &ips, &juego, &[&sha(&rom)]);
        assert!(fs::read(r.unwrap_or_else(|e| panic!("{e}\n{log:?}"))).unwrap() == traducido(&con));
        assert!(log.iter().any(|l| l.contains("(SHA-1 ")) && !log.iter().any(|l| l.contains("Aviso")), "{log:?}");
    }

    /// Como el zip de Rondo of Blood: el mismo parche en varias carpetas, una por cada forma de volcar el
    /// disco. Se queda con el que vale para tu copia, sea cual sea.
    #[test]
    fn varios_parches_en_el_comprimido() {
        let Some(t) = Prueba::con("parcheo-varios", TODAS) else { return };
        // Dos volcados del mismo disco: en el B, el pregap de la pista 2 va a cero.
        let a = pistas_inventadas(60);
        let mut b = a.clone();
        b[1][..SECTOR * 75].fill(0);
        let (nueva_a, nueva_b) = (con_texto(&a[1], 250), con_texto(&b[1], 250));
        let imagen = a.concat();
        let imagen_nueva = [a[0].clone(), nueva_a.clone(), a[2].clone()].concat();
        let (crc_a, crc_b) = (motor::hex(crc32fast::hash(&a[1])), motor::hex(crc32fast::hash(&b[1])));

        xdelta_de(&t, "paquete/imagen_unica/Juego_ES.xdelta", &imagen, &imagen_nueva);
        // El .iso de un «ISO + WAV»: los 2048 bytes de datos de cada sector de la pista 2.
        let iso: Vec<u8> = a[1].chunks(SECTOR).flat_map(|s| s[16..16 + 2048].to_vec()).collect();
        let mut iso_nuevo = iso.clone();
        iso_nuevo[2048 * 250..2048 * 250 + 10].copy_from_slice(b"HOLA MUNDO");
        xdelta_de(&t, "paquete/iso/Juego_ES.xdelta", &iso, &iso_nuevo);
        xdelta_de(&t, "paquete/por_pistas_a/Juego_ES.xdelta", &a[1], &nueva_a);
        xdelta_de(&t, "paquete/por_pistas_b/Juego_ES.xdelta", &b[1], &nueva_b);
        // El léame dice el CRC32 de la imagen en un solo fichero y el del .iso, que la ficha no trae.
        let leeme = format!("imagen_unica    Fichero: la imagen entera.  CRC32 {}\r\niso    Fichero: el .iso.  CRC32 {}\r\n", motor::hex(crc32fast::hash(&imagen)), motor::hex(crc32fast::hash(&iso)));
        t.fichero("paquete/LEEME.txt", leeme.as_bytes());
        let zip = t.dir.join("Juego_ES.zip");
        let (ok, txt) = herramienta(t.her(SIETE_Z), [OsStr::new("a"), zip.as_os_str(), t.dir.join("paquete").join("*").as_os_str()]).unwrap();
        assert!(ok, "{txt}");
        let ficha = [crc_a.as_str(), crc_b.as_str()];

        // Pistas sueltas del volcado B: el cuarto parche del zip.
        let cue = disco_de_pistas(&t, "b", &b);
        let (r, log) = t.traducir("pcenginecd", &zip, &cue, &ficha);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        iguales(&contenido_chd(&t, &fin, true), &[b[0].clone(), nueva_b, b[2].clone()]);
        let i = log.iter().position(|l| l.starts_with("OK ")).unwrap();
        assert!(log[i - 1].starts_with(&sep("+ por_pistas_b\\Juego_ES.xdelta [xdelta]")), "{log:?}");

        // Pistas sueltas del volcado A, y el mismo disco en .chd.
        let traducido_a = [a[0].clone(), nueva_a, a[2].clone()];
        let cue = disco_de_pistas(&t, "a", &a);
        let (r, log) = t.traducir("pcenginecd", &zip, &cue, &ficha);
        iguales(&contenido_chd(&t, &r.unwrap_or_else(|e| panic!("{e}\n{log:?}")), true), &traducido_a);
        let chd = chd_de(&t, &disco_de_pistas(&t, "c", &a), "chd/Juego.chd");
        let (r, log) = t.traducir("pcenginecd", &zip, &chd, &ficha);
        iguales(&contenido_chd(&t, &r.unwrap_or_else(|e| panic!("{e}\n{log:?}")), true), &traducido_a);

        // El disco entero en un solo .bin: el primer parche del zip.
        t.fichero("imagen/Juego.bin", &imagen);
        let cue = t.fichero("imagen/Juego.cue", CUE_IMAGEN.as_bytes());
        let (r, log) = t.traducir("pcenginecd", &zip, &cue, &ficha);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        iguales(&contenido_chd(&t, &fin, false), &[imagen_nueva]);
        let i = log.iter().position(|l| l.starts_with("OK ")).unwrap();
        assert!(log[i - 1].starts_with(&sep("+ imagen_unica\\Juego_ES.xdelta [xdelta]")), "{log:?}");

        // «ISO + WAV»: la pista de datos es un .iso de 2048 bytes por sector y las de audio, .wav.
        t.fichero("iso/Juego 01.wav", &wav_de(&a[0]));
        t.fichero("iso/Juego 02.iso", &iso);
        t.fichero("iso/Juego 03.wav", &wav_de(&a[2]));
        let cue = t.fichero("iso/Juego.cue", CUE_ISO_WAV.as_bytes());
        let (r, log) = t.traducir("pcenginecd", &zip, &cue, &ficha);
        let fin = r.unwrap_or_else(|e| panic!("{e}\n{log:?}"));
        iguales(&contenido_chd(&t, &fin, true), &[a[0].clone(), iso_nuevo, a[2].clone()]);
        assert!(log.iter().any(|l| l.starts_with(&sep("+ iso\\Juego_ES.xdelta [xdelta]"))), "{log:?}");

        // Otro disco, en .chd (de ahí no sale ningún CRC32 con el que rechazarlo antes): se prueban los
        // cuatro parches, ninguno vale y no se crea nada.
        let chd = chd_de(&t, &disco_de_pistas(&t, "otro", &pistas_inventadas(80)), "otro_chd/Juego.chd");
        let (r, log) = t.traducir("pcenginecd", &zip, &chd, &ficha);
        assert!(r.unwrap_err().contains("no es la versión"), "{log:?}");
        assert_eq!(log.iter().filter(|l| l.starts_with("ERROR ")).count(), 4, "{log:?}");
        assert!(!t.dir.join("otro_chd").join("Juego [T-Es].chd").exists());
    }

    #[test]
    fn nombres() {
        assert_eq!(sin_etiquetas("Juego (USA) [T-En by X] [!]"), "Juego (USA) [!]");
        assert_eq!(sin_etiquetas("Juego [T+Spa1.0]"), "Juego");
        assert_eq!(sin_etiquetas("Tetris [Tengen]"), "Tetris [Tengen]");
    }
}
