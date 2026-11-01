//! El catálogo de traducciones (traducciones_fan.csv), sus portadas y descargas directas
//! (centro_datos.json) y lo que ya está traducido en tu RetroBat.

use crate::rutas::Rutas;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Mismo orden que las columnas de la página (F en app.js).
pub const CAMPOS: [&str; 14] = [
    "sistema", "juego", "titulo_original", "grupo", "version", "fecha", "estado", "formato_parche",
    "rom_requerida", "crc32", "sha1", "url", "fuente", "notas",
];
pub const SISTEMA: usize = 0;
pub const JUEGO: usize = 1;
pub const GRUPO: usize = 3;
pub const VERSION: usize = 4;
pub const CRC: usize = 9;

#[derive(Default)]
pub struct Datos {
    firma: (u128, u128),
    /// `{"filas":[...],"centro":{...}` sin cerrar: lo que cambia en cada petición se añade al final.
    base: String,
    pub filas: HashMap<String, Vec<String>>,
    pub centro: Value,
}

fn mtime(p: &Path) -> u128 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

pub fn leer_texto(p: &Path) -> std::io::Result<String> {
    let s = fs::read_to_string(p)?;
    Ok(s.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(s))
}

pub fn clave(a: &[String]) -> String {
    format!("{}|{}|{}", a[SISTEMA], a[JUEGO], a[GRUPO])
}

impl Datos {
    /// Vuelve a leer el catálogo solo si ha cambiado desde la última vez.
    pub fn cargar(&mut self, r: &Rutas) -> Result<(), String> {
        // El que lleva dentro el programa o uno más nuevo descargado de GitHub (catalogo.rs).
        let (csv_, centro) = crate::catalogo::activo(r);
        let firma = (mtime(&csv_), mtime(&centro));
        if firma == self.firma && !self.base.is_empty() {
            return Ok(());
        }
        let txt = leer_texto(&csv_).map_err(|e| format!("No puedo leer el catálogo ({}): {e}", csv_.display()))?;
        let mut rd = csv::ReaderBuilder::new().flexible(true).from_reader(txt.as_bytes());
        let cab: Vec<String> = rd.headers().map_err(|e| e.to_string())?.iter().map(str::to_string).collect();
        let pos: Vec<Option<usize>> = CAMPOS.iter().map(|c| cab.iter().position(|h| h == c)).collect();
        let mut lista = Vec::new();
        let mut filas = HashMap::new();
        for rec in rd.records() {
            let rec = rec.map_err(|e| format!("El catálogo está mal formado: {e}"))?;
            let a: Vec<String> = pos.iter().map(|p| p.and_then(|i| rec.get(i)).unwrap_or("").to_string()).collect();
            filas.insert(clave(&a), a.clone());
            lista.push(a);
        }
        self.centro = leer_texto(&centro)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| serde_json::json!({"sistemas": {}, "filas": {}}));
        self.base = format!(
            r#"{{"filas":{},"centro":{}"#,
            serde_json::to_string(&lista).unwrap_or_default(),
            serde_json::to_string(&self.centro).unwrap_or_default()
        );
        self.filas = filas;
        self.firma = firma;
        Ok(())
    }

    pub fn json(&self, r: &Rutas) -> String {
        format!(r#"{},"traducidos":{}}}"#, self.base, serde_json::to_string(&traducidos(r)).unwrap_or_default())
    }

    /// Lo que guarda centro_datos.json de una ficha: portada (p, ps, pt) y descargas (d).
    pub fn extra(&self, clave: &str) -> Option<&Value> {
        self.centro.get("filas").and_then(|f| f.get(clave))
    }
}

/// Lo traducido desde el centro o la biblioteca (clave de la ficha -> fichero), si sigue existiendo.
fn traducidos(r: &Rutas) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Ok(t) = leer_texto(&r.traducidos) {
        for l in t.lines() {
            let mut f = l.split('\t');
            if let (Some(c), Some(p)) = (f.next(), f.next()) {
                if Path::new(p).is_file() {
                    out.insert(c.to_string(), p.to_string());
                }
            }
        }
    }
    out
}

/// Un traducido renombrado: en traducidos.tsv o hechos.tsv, la ruta vieja pasa a ser la nueva.
pub fn cambiar_ruta(fichero: &Path, vieja: &Path, nueva: &Path) {
    let Ok(t) = leer_texto(fichero) else { return };
    let (vieja, nueva) = (vieja.display().to_string(), nueva.display().to_string());
    let lineas: Vec<String> = t
        .lines()
        .map(|l| {
            let mut f: Vec<&str> = l.split('\t').collect();
            if f.get(1) == Some(&vieja.as_str()) {
                f[1] = &nueva;
            }
            f.join("\t")
        })
        .collect();
    let nuevo = fichero.with_extension("tsv.nuevo");
    if fs::write(&nuevo, lineas.join("\n") + "\n").is_ok() {
        let _ = fs::rename(&nuevo, fichero);
    }
}

/// Algunos gestores de juegos dejan en las carpetas de ROMs, en lugar del juego, un fichero de texto de
/// ~180 bytes con un enlace. No son juegos.
pub fn marcador_enlace(p: &Path, len: u64) -> bool {
    if len >= 64 * 1024 {
        return false;
    }
    let mut b = [0u8; 8];
    let n = fs::File::open(p).and_then(|mut f| f.read(&mut b)).unwrap_or(0);
    b[..n].starts_with(b"http://") || b[..n].starts_with(b"https://")
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn cambia_solo_la_ruta_vieja() {
        let d = crate::motor::pruebas::carpeta("datos-cambiar-ruta");
        let f = d.join("traducidos.tsv");
        fs::write(&f, "gamegear|A\tC:\\J\\A [T-Es].bin\t1\nsnes|B\tC:\\J\\B [T-Es].smc\t2\n").unwrap();
        cambiar_ruta(&f, Path::new("C:\\J\\A [T-Es].bin"), Path::new("C:\\J\\A [T-Es].gg"));
        assert_eq!(fs::read_to_string(&f).unwrap(), "gamegear|A\tC:\\J\\A [T-Es].gg\t1\nsnes|B\tC:\\J\\B [T-Es].smc\t2\n");
    }
}
