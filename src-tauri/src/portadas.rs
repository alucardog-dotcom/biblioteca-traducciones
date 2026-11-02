//! Portadas que pone el usuario a mano: una imagen de su disco o pegada desde el portapapeles, que se
//! copia a <datos>\portadas, o un enlace a una imagen de internet. Se guardan por juego en
//! <datos>\portadas_propias.json, con la misma clave de juego que usa la ventana para juntar sus traducciones.

use crate::rutas::Rutas;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const EXTENSIONES: [&str; 7] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "avif"];
const MAX_BYTES: u64 = 20 * 1024 * 1024;

fn fichero_mapa(r: &Rutas) -> PathBuf {
    r.datos.join("portadas_propias.json")
}

pub fn leer(r: &Rutas) -> Value {
    fs::read(fichero_mapa(r)).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

fn guardar(r: &Rutas, clave: &str, entrada: Option<Value>) -> Result<Value, String> {
    let mut m = leer(r);
    borrar_copia(r, m.get(clave));
    let o = m.as_object_mut().expect("el mapa es un objeto");
    match entrada {
        Some(e) => o.insert(clave.to_string(), e),
        None => o.remove(clave),
    };
    crate::datos::escribir_atomico(&fichero_mapa(r), &serde_json::to_vec_pretty(&m).unwrap_or_default()).map_err(|e| format!("No se pudo guardar la portada: {e}"))?;
    Ok(m)
}

/// Nombre de fichero estable para la clave del juego (FNV-1a de 64 bits).
fn nombre(clave: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in clave.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Borra la imagen copiada de esa entrada, si la había.
fn borrar_copia(r: &Rutas, entrada: Option<&Value>) {
    if let Some(ruta) = entrada.and_then(|e| e.get("ruta")).and_then(Value::as_str) {
        let p = Path::new(ruta);
        if Rutas::dentro(p, &r.datos.join("portadas")) {
            let _ = fs::remove_file(p);
        }
    }
}

/// Tipo de imagen por sus primeros bytes.
fn tipo(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG") {
        Some("png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if b.starts_with(b"GIF8") {
        Some("gif")
    } else if b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("webp")
    } else if b.starts_with(b"BM") {
        Some("bmp")
    } else if b.len() > 12 && matches!(&b[4..12], b"ftypavif" | b"ftypavis") {
        Some("avif")
    } else {
        None
    }
}

/// Copia la imagen a <datos>\portadas con el nombre del juego. El nombre cambia en cada copia (lleva la
/// hora en milisegundos) para que la ventana no siga enseñando la anterior desde su caché, y para que dos
/// copias seguidas no se pisen (al guardar se borra la anterior).
fn copiar(r: &Rutas, clave: &str, ext: &str, escribir: impl FnOnce(&Path) -> std::io::Result<()>) -> Result<Value, String> {
    let dir = r.datos.join("portadas");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let hora = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let destino = dir.join(format!("{}-{hora}.{ext}", nombre(clave)));
    escribir(&destino).map_err(|e| format!("No se pudo guardar la imagen: {e}"))?;
    guardar(r, clave, Some(json!({"ruta": destino})))
}

/// Una imagen del disco o un enlace a una imagen de internet.
pub fn poner(r: &Rutas, clave: &str, origen: &str) -> Result<Value, String> {
    if origen.starts_with("https://") || origen.starts_with("http://") {
        if origen.chars().any(char::is_whitespace) {
            return Err("Ese enlace no es válido.".into());
        }
        return guardar(r, clave, Some(json!({"url": origen})));
    }
    let p = Path::new(origen);
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !EXTENSIONES.contains(&ext.as_str()) {
        return Err("La portada tiene que ser una imagen PNG, JPG, WEBP, GIF, BMP o AVIF.".into());
    }
    let len = fs::metadata(p).map_err(|_| "No encuentro esa imagen.".to_string())?.len();
    if len > MAX_BYTES {
        return Err("Esa imagen ocupa más de 20 MB.".into());
    }
    copiar(r, clave, &ext, |destino| fs::copy(p, destino).map(|_| ()))
}

/// Una imagen pegada desde el portapapeles.
pub fn pegar(r: &Rutas, clave: &str, datos: &[u8]) -> Result<Value, String> {
    if datos.len() as u64 > MAX_BYTES {
        return Err("Esa imagen ocupa más de 20 MB.".into());
    }
    let ext = tipo(datos).ok_or("Eso no es una imagen PNG, JPG, WEBP, GIF, BMP o AVIF.")?;
    copiar(r, clave, ext, |destino| fs::write(destino, datos))
}

pub fn quitar(r: &Rutas, clave: &str) -> Result<Value, String> {
    guardar(r, clave, None)
}

/// La clave del juego llega en una cabecera, codificada con encodeURIComponent.
pub fn descodificar(s: &str) -> String {
    percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned()
}
