//! Motor de parches: IPS, BPS, UPS, PPF (1, 2 y 3) y APS (N64 y GBA). Los xdelta los aplica xdelta3.exe
//! (parcheo.rs). Es el PatchEngine.cs de herramientas pasado a Rust, con los mismos formatos y las mismas
//! comprobaciones, para no depender de PowerShell.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Límite de los formatos que trabajan en memoria (BPS, UPS y APS de GBA).
const MAX_EN_MEMORIA: u64 = 1800 * 1024 * 1024;
/// Tamaño final máximo que se admite a un parche APS de N64 (el cartucho más grande tiene 64 MB).
const MAX_N64: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formato {
    Ips,
    Bps,
    Ups,
    Ppf(u8),
    ApsN64,
    ApsGba,
    Xdelta,
}

impl Formato {
    pub fn nombre(self) -> &'static str {
        match self {
            Formato::Ips => "ips",
            Formato::Bps => "bps",
            Formato::Ups => "ups",
            Formato::Ppf(_) => "ppf",
            Formato::ApsN64 | Formato::ApsGba => "aps",
            Formato::Xdelta => "xdelta",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Comprobacion {
    Vale,
    NoCoincide,
    YaParcheado,
    /// El formato no permite comprobarlo.
    Desconocido,
}

#[derive(Default)]
pub struct Info {
    pub formato: Option<Formato>,
    /// BPS y UPS llevan el CRC32 del juego original y el del traducido.
    pub crc_origen: Option<u32>,
    pub crc_destino: Option<u32>,
    /// PPF 2 y 3: un bloque del disco original para comprobar que es la versión buena.
    ppf_bloque: Option<(u64, Vec<u8>)>,
    ppf_tam_entrada: Option<u64>,
}

pub fn hex(c: u32) -> String {
    format!("{c:08X}")
}

fn leer_inicio(p: &Path, n: usize) -> io::Result<Vec<u8>> {
    let mut b = Vec::with_capacity(n);
    File::open(p)?.take(n as u64).read_to_end(&mut b)?;
    Ok(b)
}

fn u32le(b: &[u8], p: usize) -> Result<u32, String> {
    b.get(p..p + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]])).ok_or_else(|| "Parche truncado o dañado.".to_string())
}

fn u64le(b: &[u8], p: usize) -> Result<u64, String> {
    Ok(u32le(b, p)? as u64 | (u32le(b, p + 4)? as u64) << 32)
}

/// Número de longitud variable de BPS y UPS.
fn varint(p: &[u8], pos: &mut usize) -> Result<u64, String> {
    let (mut dato, mut paso) = (0u64, 1u64);
    loop {
        let x = *p.get(*pos).ok_or("Parche truncado o dañado.")?;
        *pos += 1;
        dato = dato.wrapping_add((x & 0x7f) as u64 * paso);
        if x & 0x80 != 0 {
            return Ok(dato);
        }
        if paso > 1 << 56 {
            return Err("Parche dañado.".into());
        }
        paso <<= 7;
        dato = dato.wrapping_add(paso);
    }
}

pub fn detectar(parche: &Path) -> Option<Formato> {
    let h = leer_inicio(parche, 8).ok()?;
    Some(if h.starts_with(b"PATCH") {
        Formato::Ips
    } else if h.starts_with(b"BPS1") {
        Formato::Bps
    } else if h.starts_with(b"UPS1") {
        Formato::Ups
    } else if h.starts_with(b"PPF10") {
        Formato::Ppf(1)
    } else if h.starts_with(b"PPF20") {
        Formato::Ppf(2)
    } else if h.starts_with(b"PPF30") {
        Formato::Ppf(3)
    } else if h.starts_with(b"APS10") {
        Formato::ApsN64
    } else if h.starts_with(b"APS1") {
        Formato::ApsGba
    } else if h.starts_with(&[0xD6, 0xC3, 0xC4, 0x00]) {
        Formato::Xdelta
    } else {
        return None;
    })
}

pub fn inspeccionar(parche: &Path) -> Result<Info, String> {
    let mut info = Info { formato: detectar(parche), ..Default::default() };
    match info.formato {
        Some(Formato::Bps | Formato::Ups) => {
            let p = fs::read(parche).map_err(|e| e.to_string())?;
            if p.len() < 16 {
                return Err("Parche demasiado pequeño.".into());
            }
            info.crc_origen = Some(u32le(&p, p.len() - 12)?);
            info.crc_destino = Some(u32le(&p, p.len() - 8)?);
        }
        Some(Formato::Ppf(v)) => {
            let h = leer_inicio(parche, 1084).map_err(|e| e.to_string())?;
            if h.len() >= 1084 {
                if v == 2 {
                    info.ppf_tam_entrada = Some(u32le(&h, 56)? as u64);
                    info.ppf_bloque = Some((0x9320, h[60..1084].to_vec()));
                } else if v == 3 && h[57] != 0 {
                    info.ppf_bloque = Some((if h[56] == 0 { 0x9320 } else { 0x80A0 }, h[60..1084].to_vec()));
                }
            }
        }
        _ => {}
    }
    Ok(info)
}

/// CRC32 de un fichero saltando `salto` bytes del principio (la cabecera de copiador de SNES, por ejemplo).
pub fn crc_fichero(p: &Path, salto: u64) -> io::Result<u32> {
    let mut f = File::open(p)?;
    f.seek(SeekFrom::Start(salto))?;
    let mut h = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(h.finalize());
        }
        h.update(&buf[..n]);
    }
}

pub fn comprobar(info: &Info, origen: &Path, salto: u64) -> Comprobacion {
    if let (Some(o), Some(d)) = (info.crc_origen, info.crc_destino) {
        return match crc_fichero(origen, salto) {
            Ok(c) if c == o => Comprobacion::Vale,
            Ok(c) if c == d => Comprobacion::YaParcheado,
            _ => Comprobacion::NoCoincide,
        };
    }
    if let Some((desde, bloque)) = &info.ppf_bloque {
        let mut b = vec![0u8; 1024];
        let leido = File::open(origen).and_then(|mut f| {
            f.seek(SeekFrom::Start(*desde))?;
            f.read_exact(&mut b)
        });
        return if leido.is_ok() && &b == bloque { Comprobacion::Vale } else { Comprobacion::NoCoincide };
    }
    Comprobacion::Desconocido
}

/// Aplica el parche. `salto` quita bytes del principio del juego original (la cabecera de copiador de
/// SNES). Devuelve un texto con el resultado de la verificación.
pub fn aplicar(parche: &Path, origen: &Path, salida: &Path, salto: u64, forzar: bool) -> Result<String, String> {
    let r = match detectar(parche) {
        Some(Formato::Ips) => ips(parche, origen, salida, salto),
        Some(Formato::Bps) => bps(parche, origen, salida, salto, forzar),
        Some(Formato::Ups) => ups(parche, origen, salida, salto, forzar),
        Some(Formato::Ppf(_)) => ppf(parche, origen, salida, forzar),
        Some(Formato::ApsN64) => aps_n64(parche, origen, salida),
        Some(Formato::ApsGba) => aps_gba(parche, origen, salida),
        Some(Formato::Xdelta) => Err("Los parches xdelta se aplican con xdelta3.exe.".into()),
        None => Err("Formato de parche no reconocido.".into()),
    };
    if r.is_err() {
        let _ = fs::remove_file(salida);
    }
    r
}

fn es(e: io::Error) -> String {
    e.to_string()
}

fn copiar_origen(origen: &Path, salida: &Path, salto: u64) -> Result<(), String> {
    if salto == 0 {
        return fs::copy(origen, salida).map(|_| ()).map_err(es);
    }
    let mut o = File::open(origen).map_err(es)?;
    o.seek(SeekFrom::Start(salto)).map_err(es)?;
    io::copy(&mut o, &mut File::create(salida).map_err(es)?).map(|_| ()).map_err(es)
}

fn leer_origen(origen: &Path, salto: u64) -> Result<Vec<u8>, String> {
    let len = fs::metadata(origen).map_err(es)?.len();
    if len.saturating_sub(salto) > MAX_EN_MEMORIA {
        return Err("El juego es demasiado grande para este formato de parche.".into());
    }
    let mut b = fs::read(origen).map_err(es)?;
    b.drain(..(salto as usize).min(b.len()));
    Ok(b)
}

fn abrir_rw(p: &Path) -> Result<File, String> {
    OpenOptions::new().read(true).write(true).open(p).map_err(es)
}

fn escribir_en(f: &mut File, desde: u64, datos: &[u8]) -> Result<(), String> {
    f.seek(SeekFrom::Start(desde)).map_err(es)?;
    f.write_all(datos).map_err(es)
}

fn ips(parche: &Path, origen: &Path, salida: &Path, salto: u64) -> Result<String, String> {
    let p = fs::read(parche).map_err(es)?;
    copiar_origen(origen, salida, salto)?;
    let mut o = abrir_rw(salida)?;
    let truncado = || "Parche IPS truncado.".to_string();
    let u24 = |i: usize| p.get(i..i + 3).map(|x| (x[0] as u64) << 16 | (x[1] as u64) << 8 | x[2] as u64);
    let (mut pos, mut bloques) = (5usize, 0u32);
    loop {
        let desde = u24(pos).ok_or("Parche IPS truncado (falta EOF).")?;
        pos += 3;
        if desde == 0x454F46 {
            // Tras «EOF» pueden venir exactamente 3 bytes: el tamaño final del juego (truncado). Cualquier
            // otra cosa sobrante (relleno, otro formato pegado) no es un tamaño y se ignora.
            if p.len() == pos + 3 {
                if let Some(t) = u24(pos) {
                    o.set_len(t).map_err(es)?;
                }
            }
            break;
        }
        let tam = p.get(pos..pos + 2).map(|x| (x[0] as usize) << 8 | x[1] as usize).ok_or_else(truncado)?;
        pos += 2;
        if tam == 0 {
            let x = p.get(pos..pos + 3).ok_or_else(truncado)?;
            let (n, valor) = ((x[0] as usize) << 8 | x[1] as usize, x[2]);
            pos += 3;
            escribir_en(&mut o, desde, &vec![valor; n])?;
        } else {
            escribir_en(&mut o, desde, p.get(pos..pos + tam).ok_or_else(truncado)?)?;
            pos += tam;
        }
        bloques += 1;
    }
    Ok(format!("IPS aplicado ({bloques} bloques). El formato IPS no permite verificar el resultado."))
}

fn comprobar_crc_parche(p: &[u8], formato: &str) -> Result<(), String> {
    let n = p.len() - 4;
    if crc32fast::hash(&p[..n]) != u32le(p, n)? {
        return Err(format!("El fichero {formato} está dañado (CRC del parche incorrecto)."));
    }
    Ok(())
}

/// None si el juego es el que pide el parche; una nota si se ha forzado; error si no vale.
fn comprobar_crc_origen(real: u32, origen: u32, destino: u32, forzar: bool) -> Result<String, String> {
    if real == origen {
        Ok(String::new())
    } else if real == destino {
        Err("Este juego ya tiene aplicado este parche.".into())
    } else if !forzar {
        Err(format!("El juego no es la versión que pide el parche (CRC32 {}, se esperaba {}).", hex(real), hex(origen)))
    } else {
        Ok(format!(" (forzado: CRC de origen {} en vez de {})", hex(real), hex(origen)))
    }
}

fn reservar(tam: u64) -> Result<Vec<u8>, String> {
    if tam > MAX_EN_MEMORIA {
        return Err("El resultado sería demasiado grande.".into());
    }
    Ok(vec![0u8; tam as usize])
}

fn bps(parche: &Path, origen: &Path, salida: &Path, salto: u64, forzar: bool) -> Result<String, String> {
    let p = fs::read(parche).map_err(es)?;
    if p.len() < 19 {
        return Err("Parche BPS demasiado pequeño.".into());
    }
    comprobar_crc_parche(&p, "BPS")?;
    let mut pos = 4;
    let _tam_origen = varint(&p, &mut pos)?;
    let tam_destino = varint(&p, &mut pos)?;
    let meta = varint(&p, &mut pos)?;
    pos = pos.saturating_add(meta as usize);
    let (crc_o, crc_d, fin) = (u32le(&p, p.len() - 12)?, u32le(&p, p.len() - 8)?, p.len() - 12);

    let src = leer_origen(origen, salto)?;
    let nota = comprobar_crc_origen(crc32fast::hash(&src), crc_o, crc_d, forzar)?;
    let mut tgt = reservar(tam_destino)?;
    let no_encaja = || "El parche BPS no encaja con este juego.".to_string();
    let (mut sal, mut rel_o, mut rel_d) = (0usize, 0i64, 0i64);
    while pos < fin {
        let dato = varint(&p, &mut pos)?;
        let n = (dato >> 2) as usize + 1;
        if sal + n > tgt.len() {
            return Err(no_encaja());
        }
        match dato & 3 {
            // Copiar del original en la misma posición.
            0 => {
                for i in sal..sal + n {
                    tgt[i] = src.get(i).copied().unwrap_or(0);
                }
            }
            // Bytes nuevos que trae el parche.
            1 => {
                tgt[sal..sal + n].copy_from_slice(p.get(pos..pos + n).ok_or_else(no_encaja)?);
                pos += n;
            }
            // Copiar de otra parte del original o de lo ya escrito.
            modo => {
                let d = varint(&p, &mut pos)?;
                let salto_rel = if d & 1 != 0 { -((d >> 1) as i64) } else { (d >> 1) as i64 };
                let rel = if modo == 2 { &mut rel_o } else { &mut rel_d };
                *rel += salto_rel;
                if *rel < 0 {
                    return Err(no_encaja());
                }
                for i in sal..sal + n {
                    let j = *rel as usize;
                    tgt[i] = if modo == 2 { *src.get(j).ok_or_else(no_encaja)? } else { *tgt.get(j).filter(|_| j < i).ok_or_else(no_encaja)? };
                    *rel += 1;
                }
            }
        }
        sal += n;
    }
    if crc32fast::hash(&tgt) != crc_d {
        return Err("El resultado no supera la verificación CRC32; no se ha guardado.".into());
    }
    fs::write(salida, &tgt).map_err(es)?;
    Ok(format!("BPS aplicado y verificado (CRC32 {}){nota}.", hex(crc_d)))
}

fn ups(parche: &Path, origen: &Path, salida: &Path, salto: u64, forzar: bool) -> Result<String, String> {
    let p = fs::read(parche).map_err(es)?;
    if p.len() < 18 {
        return Err("Parche UPS demasiado pequeño.".into());
    }
    comprobar_crc_parche(&p, "UPS")?;
    let mut pos = 4;
    let _tam_origen = varint(&p, &mut pos)?;
    let tam_destino = varint(&p, &mut pos)?;
    let (crc_o, crc_d, fin) = (u32le(&p, p.len() - 12)?, u32le(&p, p.len() - 8)?, p.len() - 12);

    let src = leer_origen(origen, salto)?;
    let nota = comprobar_crc_origen(crc32fast::hash(&src), crc_o, crc_d, forzar)?;
    let mut tgt = reservar(tam_destino)?;
    let n = src.len().min(tgt.len());
    tgt[..n].copy_from_slice(&src[..n]);
    let mut rel = 0u64;
    while pos < fin {
        rel = rel.saturating_add(varint(&p, &mut pos)?);
        loop {
            if pos >= fin {
                return Err("Parche UPS truncado.".into());
            }
            let x = p[pos];
            pos += 1;
            if let Some(b) = tgt.get_mut(rel as usize) {
                *b = src.get(rel as usize).copied().unwrap_or(0) ^ x;
            }
            rel += 1;
            if x == 0 {
                break;
            }
        }
    }
    if crc32fast::hash(&tgt) != crc_d {
        return Err("El resultado no supera la verificación CRC32; no se ha guardado.".into());
    }
    fs::write(salida, &tgt).map_err(es)?;
    Ok(format!("UPS aplicado y verificado (CRC32 {}){nota}.", hex(crc_d)))
}

/// Dónde empieza la descripción FILE_ID.DIZ del final de un PPF (o el final, si no la lleva).
fn inicio_diz(p: &[u8], datos: usize) -> usize {
    let (abre, cierra): (&[u8], &[u8]) = (b"@BEGIN_FILE_ID.DIZ", b"@END_FILE_ID.DIZ");
    // La etiqueta de cierre va seguida de 2 (PPF3) o 4 (PPF2) bytes de longitud.
    let con_cierre = [2usize, 4].iter().any(|&cola| {
        p.len().checked_sub(cola + cierra.len()).is_some_and(|en| en >= datos && &p[en..en + cierra.len()] == cierra)
    });
    if !con_cierre || p.len() < abre.len() {
        return p.len();
    }
    let desde = datos.max(p.len().saturating_sub(70000));
    (desde..=p.len() - abre.len()).rev().find(|&i| &p[i..i + abre.len()] == abre).unwrap_or(p.len())
}

fn ppf(parche: &Path, origen: &Path, salida: &Path, forzar: bool) -> Result<String, String> {
    let info = inspeccionar(parche)?;
    let p = fs::read(parche).map_err(es)?;
    let mut nota = "";
    if comprobar(&info, origen, 0) == Comprobacion::NoCoincide {
        if !forzar {
            return Err("El disco no es la versión que pide el parche PPF (falla la comprobación de bloque).".into());
        }
        nota = " (forzado: la comprobación de bloque fallaba)";
    }
    if let Some(t) = info.ppf_tam_entrada.filter(|&t| t > 0) {
        if fs::metadata(origen).map_err(es)?.len() != t && !forzar {
            return Err("El tamaño del disco no coincide con el que pide el parche PPF2.".into());
        }
    }
    let version = match info.formato {
        Some(Formato::Ppf(v)) => v,
        _ => 0,
    };
    let ppf3 = version == 3;
    let deshacer = ppf3 && p.len() > 58 && p[58] != 0;
    let mut pos = match version {
        1 => 56,
        2 => 1084,
        _ if p.len() > 57 && p[57] != 0 => 1084,
        _ => 60,
    };
    let fin = inicio_diz(&p, pos);

    // Si todos los bloques ya están en el original, el disco ya estaba traducido.
    let (mut bloques, mut iguales) = (0u32, 0u32);
    fs::copy(origen, salida).map_err(es)?;
    let mut o = abrir_rw(salida)?;
    let largo = o.metadata().map_err(es)?.len();
    let mut actual = [0u8; 256];
    while pos < fin {
        let desde = if ppf3 {
            if pos + 9 > fin {
                break;
            }
            pos += 8;
            u64le(&p, pos - 8)?
        } else {
            if pos + 5 > fin {
                break;
            }
            pos += 4;
            u32le(&p, pos - 4)? as u64
        };
        let n = p[pos] as usize;
        pos += 1;
        if pos + n > fin {
            return Err("Parche PPF truncado.".into());
        }
        let nuevo = &p[pos..pos + n];
        // Un PPF escribe dentro del disco (como mucho, justo al final): un desplazamiento mayor es un parche
        // corrupto o de otra imagen, no una orden de agrandar el fichero gigabytes. Va antes de la suma de
        // abajo: con un desplazamiento de PPF3 cerca de u64::MAX, `desde + n` se desbordaría.
        if desde > largo {
            return Err("El parche PPF apunta fuera del disco: no es para esta imagen o está dañado.".into());
        }
        if desde + n as u64 <= largo {
            o.seek(SeekFrom::Start(desde)).map_err(es)?;
            o.read_exact(&mut actual[..n]).map_err(es)?;
            if &actual[..n] == nuevo {
                iguales += 1;
            }
        }
        escribir_en(&mut o, desde, nuevo)?;
        pos += n;
        if deshacer {
            pos += n;
        }
        bloques += 1;
    }
    drop(o);
    if bloques == 0 {
        return Err("El parche PPF no tiene ningún bloque: está truncado o dañado.".into());
    }
    if iguales == bloques {
        return Err("Este disco ya tiene aplicado este parche.".into());
    }
    let que = if info.ppf_bloque.is_some() { "PPF aplicado (bloque de comprobación correcto)" } else { "PPF aplicado (este PPF no incluye comprobación)" };
    Ok(format!("{que}, {bloques} bloques{nota}."))
}

fn aps_n64(parche: &Path, origen: &Path, salida: &Path) -> Result<String, String> {
    let p = fs::read(parche).map_err(es)?;
    if p.len() < 61 {
        return Err("Parche APS demasiado pequeño.".into());
    }
    let mut pos = 7 + 50;
    let mut tam_final = None;
    if p[5] == 1 {
        // Formato original, ID del cartucho, país, CRC y relleno; después, el tamaño final.
        pos += 1 + 3 + 1 + 8 + 5;
        tam_final = Some(u32le(&p, pos)? as u64);
        pos += 4;
    }
    fs::copy(origen, salida).map_err(es)?;
    let mut o = abrir_rw(salida)?;
    if let Some(t) = tam_final.filter(|&t| t > 0) {
        // Ningún cartucho de N64 pasa de 64 MB: un tamaño final mayor es un parche dañado.
        if t > MAX_N64 {
            return Err("El parche APS pide un tamaño final imposible para un juego de N64.".into());
        }
        o.set_len(t).map_err(es)?;
    }
    let truncado = || "Parche APS truncado.".to_string();
    // Tampoco se escribe más allá de lo que puede ocupar un cartucho: sin esto, un bloque en 0xFFFFFF00 de un
    // APS sin tamaño final crearía un fichero de 4 GB.
    let fuera = || "El parche APS escribe fuera de cualquier juego de N64: está dañado.".to_string();
    while pos + 5 <= p.len() {
        let desde = u32le(&p, pos)? as u64;
        let n = p[pos + 4] as usize;
        pos += 5;
        if n == 0 {
            let x = p.get(pos..pos + 2).ok_or_else(truncado)?;
            if desde + x[1] as u64 > MAX_N64 {
                return Err(fuera());
            }
            escribir_en(&mut o, desde, &vec![x[0]; x[1] as usize])?;
            pos += 2;
        } else {
            if desde + n as u64 > MAX_N64 {
                return Err(fuera());
            }
            escribir_en(&mut o, desde, p.get(pos..pos + n).ok_or_else(truncado)?)?;
            pos += n;
        }
    }
    Ok("APS (N64) aplicado. Este formato no permite verificar el resultado.".into())
}

fn aps_gba(parche: &Path, origen: &Path, salida: &Path) -> Result<String, String> {
    let p = fs::read(parche).map_err(es)?;
    if p.len() < 12 {
        return Err("Parche APS demasiado pequeño.".into());
    }
    let (tam_origen, tam_destino) = (u32le(&p, 4)? as usize, u32le(&p, 8)? as u64);
    let src = leer_origen(origen, 0)?;
    if src.len() != tam_origen {
        return Err("El tamaño del juego no coincide con el que pide el parche APS.".into());
    }
    let mut tgt = reservar(tam_destino)?;
    let n = src.len().min(tgt.len());
    tgt[..n].copy_from_slice(&src[..n]);
    const BLOQUE: usize = 0x10000;
    let mut pos = 12;
    while pos + 8 + BLOQUE <= p.len() {
        // Desplazamiento y dos CRC16 del bloque.
        let desde = u32le(&p, pos)? as usize;
        pos += 8;
        for i in 0..BLOQUE {
            let j = desde + i;
            if j >= tgt.len() {
                break;
            }
            tgt[j] = src.get(j).copied().unwrap_or(0) ^ p[pos + i];
        }
        pos += BLOQUE;
    }
    fs::write(salida, &tgt).map_err(es)?;
    Ok("APS (GBA) aplicado.".into())
}

#[cfg(test)]
pub(crate) mod pruebas {
    //! Parches hechos a mano para cada formato, y comprobaciones de que el motor los aplica bien.
    use super::*;
    use std::path::PathBuf;

    pub fn carpeta(nombre: &str) -> PathBuf {
        let d = std::env::temp_dir().join("biblioteca-pruebas").join(nombre);
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Bytes pseudoaleatorios (siempre los mismos para la misma semilla).
    pub fn ruido(n: usize, semilla: u32) -> Vec<u8> {
        let mut x = semilla.wrapping_mul(2654435761).max(1);
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    pub fn traducido(src: &[u8]) -> Vec<u8> {
        let mut t = src.to_vec();
        t[100..110].copy_from_slice(b"HOLA MUNDO");
        t[5000..5003].copy_from_slice(&[1, 2, 3]);
        // Una repetición de lo ya escrito, para la orden de BPS que copia del propio resultado.
        t[0] ^= 0xff;
        t.copy_within(0..4, 200);
        t.extend_from_slice(b"ESPANOL");
        t
    }

    fn vint(mut n: u64, out: &mut Vec<u8>) {
        loop {
            let x = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(x | 0x80);
                return;
            }
            out.push(x);
            n -= 1;
        }
    }

    fn cierre(mut p: Vec<u8>, src: &[u8], tgt: &[u8]) -> Vec<u8> {
        p.extend_from_slice(&crc32fast::hash(src).to_le_bytes());
        p.extend_from_slice(&crc32fast::hash(tgt).to_le_bytes());
        let c = crc32fast::hash(&p);
        p.extend_from_slice(&c.to_le_bytes());
        p
    }

    pub fn ips_de(src: &[u8], tgt: &[u8]) -> Vec<u8> {
        let mut p = b"PATCH".to_vec();
        let mut i = 0;
        while i < tgt.len() {
            if src.get(i) == Some(&tgt[i]) {
                i += 1;
                continue;
            }
            let ini = i;
            while i < tgt.len() && src.get(i) != Some(&tgt[i]) && i - ini < 0xffff {
                i += 1;
            }
            p.extend_from_slice(&(ini as u32).to_be_bytes()[1..]);
            p.extend_from_slice(&((i - ini) as u16).to_be_bytes());
            p.extend_from_slice(&tgt[ini..i]);
        }
        // Un bloque RLE para probar también ese caso (rellena con lo que ya hay).
        p.extend_from_slice(&[0, 0, 0, 0, 0, 0, 1, tgt[0]]);
        p.extend_from_slice(b"EOF");
        p
    }

    /// BPS con las cuatro órdenes: copiar del original, bytes nuevos, copiar de otra parte del original
    /// y repetir lo ya escrito.
    pub fn bps_de(src: &[u8], tgt: &[u8]) -> Vec<u8> {
        let mut p = b"BPS1".to_vec();
        vint(src.len() as u64, &mut p);
        vint(tgt.len() as u64, &mut p);
        vint(0, &mut p);
        let orden = |p: &mut Vec<u8>, modo: u64, n: usize| vint(((n as u64 - 1) << 2) | modo, p);
        let rel = |p: &mut Vec<u8>, d: i64| vint(((d.unsigned_abs()) << 1) | (d < 0) as u64, p);
        // Lo igual se copia del original; en la posición 200 se repite lo escrito al principio; de lo
        // cambiado, un byte de cada tres va en el parche y los demás se copian de otra parte del original.
        let (mut sal, mut rel_o, mut rel_d) = (0usize, 0i64, 0i64);
        while sal < tgt.len() {
            if sal == 200 && tgt[200..204] == tgt[0..4] {
                orden(&mut p, 3, 4);
                rel(&mut p, 0 - rel_d);
                rel_d = 4;
                sal += 4;
            } else if src.get(sal) == Some(&tgt[sal]) {
                let ini = sal;
                while sal < tgt.len() && src.get(sal) == Some(&tgt[sal]) && sal != 200 {
                    sal += 1;
                }
                orden(&mut p, 0, sal - ini);
            } else if let Some(j) = src.iter().position(|&b| b == tgt[sal]).filter(|_| sal % 3 != 0) {
                orden(&mut p, 2, 1);
                rel(&mut p, j as i64 - rel_o);
                rel_o = j as i64 + 1;
                sal += 1;
            } else {
                orden(&mut p, 1, 1);
                p.push(tgt[sal]);
                sal += 1;
            }
        }
        cierre(p, src, tgt)
    }

    pub fn ups_de(src: &[u8], tgt: &[u8]) -> Vec<u8> {
        let mut p = b"UPS1".to_vec();
        vint(src.len() as u64, &mut p);
        vint(tgt.len() as u64, &mut p);
        let (mut i, mut ultimo) = (0usize, 0usize);
        let n = src.len().max(tgt.len());
        while i < n {
            let x = src.get(i).copied().unwrap_or(0) ^ tgt.get(i).copied().unwrap_or(0);
            if x == 0 {
                i += 1;
                continue;
            }
            vint((i - ultimo) as u64, &mut p);
            while i < n {
                let x = src.get(i).copied().unwrap_or(0) ^ tgt.get(i).copied().unwrap_or(0);
                if x == 0 {
                    break;
                }
                p.push(x);
                i += 1;
            }
            p.push(0);
            i += 1;
            ultimo = i;
        }
        cierre(p, src, tgt)
    }

    /// PPF3 con bloque de comprobación en 0x9320.
    pub fn ppf3_de(src: &[u8], tgt: &[u8]) -> Vec<u8> {
        let mut p = b"PPF30".to_vec();
        p.push(2);
        p.extend_from_slice(&[b' '; 50]);
        p.extend_from_slice(&[0, 1, 0, 0]);
        p.extend_from_slice(&src[0x9320..0x9320 + 1024]);
        for i in 0..src.len() {
            if src[i] != tgt[i] {
                p.extend_from_slice(&(i as u64).to_le_bytes());
                p.push(1);
                p.push(tgt[i]);
            }
        }
        p
    }

    fn escribir(dir: &Path, nombre: &str, b: &[u8]) -> PathBuf {
        let p = dir.join(nombre);
        fs::write(&p, b).unwrap();
        p
    }

    #[test]
    fn varint_ida_y_vuelta() {
        for n in [0u64, 1, 127, 128, 255, 16383, 16384, 1 << 30, (1 << 40) + 12345] {
            let mut b = vec![];
            vint(n, &mut b);
            let mut pos = 0;
            assert_eq!(varint(&b, &mut pos).unwrap(), n);
            assert_eq!(pos, b.len());
        }
    }

    #[test]
    fn formatos() {
        let d = carpeta("motor");
        let src = ruido(40000, 7);
        let tgt = traducido(&src);
        let origen = escribir(&d, "juego.bin", &src);
        for (nombre, parche, formato) in [
            ("p.ips", ips_de(&src, &tgt), Formato::Ips),
            ("p.bps", bps_de(&src, &tgt), Formato::Bps),
            ("p.ups", ups_de(&src, &tgt), Formato::Ups),
        ] {
            let pp = escribir(&d, nombre, &parche);
            assert_eq!(detectar(&pp), Some(formato));
            let salida = d.join(format!("salida-{nombre}"));
            let msg = aplicar(&pp, &origen, &salida, 0, false).unwrap();
            assert_eq!(fs::read(&salida).unwrap(), tgt, "{nombre}: {msg}");
        }
        // PPF no cambia el tamaño: el traducido tiene la misma longitud.
        let tgt2 = traducido(&src)[..src.len()].to_vec();
        let pp = escribir(&d, "p.ppf", &ppf3_de(&src, &tgt2));
        let salida = d.join("salida.ppf.bin");
        aplicar(&pp, &origen, &salida, 0, false).unwrap();
        assert_eq!(fs::read(&salida).unwrap(), tgt2);
        // Sobre el ya traducido: lo dice.
        let ya = escribir(&d, "ya.bin", &tgt2);
        assert!(aplicar(&pp, &ya, &d.join("otra.bin"), 0, false).unwrap_err().contains("ya tiene aplicado"));
    }

    #[test]
    fn rechaza_otra_version() {
        let d = carpeta("motor-otra");
        let src = ruido(40000, 3);
        let tgt = traducido(&src);
        let otra = escribir(&d, "otra.bin", &ruido(40000, 4));
        for (nombre, parche) in [("p.bps", bps_de(&src, &tgt)), ("p.ups", ups_de(&src, &tgt))] {
            let pp = escribir(&d, nombre, &parche);
            let salida = d.join("salida.bin");
            let e = aplicar(&pp, &otra, &salida, 0, false).unwrap_err();
            assert!(e.contains("no es la versión"), "{e}");
            assert!(!salida.exists());
            assert_eq!(comprobar(&inspeccionar(&pp).unwrap(), &otra, 0), Comprobacion::NoCoincide);
        }
        let pp = escribir(&d, "p.ppf", &ppf3_de(&src, &traducido(&src)[..src.len()]));
        assert!(aplicar(&pp, &otra, &d.join("s.bin"), 0, false).unwrap_err().contains("no es la versión"));
    }

    #[test]
    fn cabecera_snes() {
        let d = carpeta("motor-cabecera");
        let src = ruido(32768, 9);
        let tgt = traducido(&src);
        let mut con = vec![0u8; 512];
        con.extend_from_slice(&src);
        let origen = escribir(&d, "juego.smc", &con);
        let pp = escribir(&d, "p.bps", &bps_de(&src, &tgt));
        assert!(aplicar(&pp, &origen, &d.join("a.sfc"), 0, false).is_err());
        aplicar(&pp, &origen, &d.join("b.sfc"), 512, false).unwrap();
        assert_eq!(fs::read(d.join("b.sfc")).unwrap(), tgt);
        assert_eq!(crc_fichero(&origen, 512).unwrap(), crc32fast::hash(&src));
    }

    #[test]
    fn parches_rotos_no_revientan() {
        let d = carpeta("motor-rotos");
        let src = ruido(20000, 5);
        let origen = escribir(&d, "juego.bin", &src);
        let buenos = [ips_de(&src, &traducido(&src)), bps_de(&src, &traducido(&src)), ups_de(&src, &traducido(&src))];
        for (k, b) in buenos.iter().enumerate() {
            for corte in [6, 10, b.len() / 2, b.len() - 1] {
                let pp = escribir(&d, &format!("roto{k}-{corte}"), &b[..corte]);
                let _ = aplicar(&pp, &origen, &d.join("s.bin"), 0, false);
            }
            for semilla in 0..20 {
                let mut x = b.clone();
                for (i, r) in ruido(x.len(), semilla).iter().enumerate().skip(8) {
                    if r % 7 == 0 {
                        x[i] ^= r;
                    }
                }
                let pp = escribir(&d, &format!("mal{k}-{semilla}"), &x);
                let _ = aplicar(&pp, &origen, &d.join("s.bin"), 0, true);
            }
        }
    }

    /// Desplazamientos imposibles: ni desbordan (PPF3 cerca de u64::MAX) ni crean ficheros de gigas (APS).
    #[test]
    fn ppf_y_aps_fuera_de_rango() {
        let d = carpeta("motor-fuera");
        let origen = escribir(&d, "juego.bin", &ruido(4096, 8));
        // PPF3 sin bloque de comprobación.
        let mut ppf = b"PPF30".to_vec();
        ppf.push(2);
        ppf.extend_from_slice(&[b' '; 50]);
        ppf.extend_from_slice(&[0, 0, 0, 0]);
        ppf.extend_from_slice(&(u64::MAX - 1).to_le_bytes());
        ppf.push(4);
        ppf.extend_from_slice(b"HOLA");
        let pp = escribir(&d, "p.ppf", &ppf);
        assert!(aplicar(&pp, &origen, &d.join("s.bin"), 0, false).unwrap_err().contains("fuera del disco"));
        // APS de N64 sin tamaño final que escribe en 0xFFFFFF00.
        let mut aps = b"APS10".to_vec();
        aps.extend_from_slice(&[0, 0]);
        aps.extend_from_slice(&[b' '; 50]);
        aps.extend_from_slice(&0xFFFF_FF00u32.to_le_bytes());
        aps.push(4);
        aps.extend_from_slice(b"HOLA");
        let pa = escribir(&d, "p.aps", &aps);
        let salida = d.join("a.bin");
        assert!(aplicar(&pa, &origen, &salida, 0, false).unwrap_err().contains("fuera de cualquier juego"));
        assert!(!salida.exists());
    }
}
