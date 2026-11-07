//! El catálogo al día sin reinstalar. El programa lleva uno dentro; al abrirse mira en GitHub si el del
//! repositorio ha cambiado (comparando el sha de git de cada fichero) y, si ha cambiado, lo descarga a
//! <datos>\catalogo y lo usa desde entonces. Cuando una versión nueva del programa trae ese mismo catálogo,
//! se vuelve a usar el que lleva dentro.

use crate::datos::leer_texto;
use crate::descarga;
use crate::rutas::Rutas;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const RUTA_REPOSITORIO: &str = "src-tauri/recursos/catalogo";
const CSV: &str = "traducciones_fan.csv";
const CENTRO: &str = "centro_datos.json";
const PROGRAMA: &str = env!("CARGO_PKG_VERSION");

/// Qué catálogo se descargó y con qué versión del programa.
#[derive(Serialize, Deserialize, Default)]
struct Estado {
    programa: String,
    sha: HashMap<String, String>,
}

fn carpeta(r: &Rutas) -> PathBuf {
    r.datos.join("catalogo")
}

fn estado(r: &Rutas) -> Option<Estado> {
    serde_json::from_slice(&fs::read(carpeta(r).join("estado.json")).ok()?).ok()
}

/// El sha que git da a un fichero (el mismo que enseña GitHub).
pub fn sha_git(p: &Path) -> Option<String> {
    let b = fs::read(p).ok()?;
    let mut h = sha1_smol::Sha1::new();
    h.update(format!("blob {}\0", b.len()).as_bytes());
    h.update(&b);
    Some(h.digest().to_string())
}

/// El catálogo que se usa (csv, centro_datos): el descargado si se bajó con esta versión del programa y
/// está entero; si no, el que lleva dentro. Así una versión nueva del programa no se queda con un
/// catálogo descargado más viejo que el suyo.
pub fn activo(r: &Rutas) -> (PathBuf, PathBuf) {
    let d = carpeta(r);
    if estado(r).is_some_and(|e| e.programa == PROGRAMA) && d.join(CSV).is_file() && d.join(CENTRO).is_file() {
        return (d.join(CSV), d.join(CENTRO));
    }
    (r.csv.clone(), r.centro.clone())
}

/// Que lo descargado sea un catálogo de verdad: un CSV con sus columnas y muchas filas, y el JSON.
fn valido(csv_: &Path, centro: &Path) -> bool {
    let filas = leer_texto(csv_).ok().map(|t| {
        let mut rd = csv::ReaderBuilder::new().flexible(true).from_reader(t.as_bytes());
        let cab_ok = rd.headers().is_ok_and(|h| h.iter().any(|c| c == "sistema") && h.iter().any(|c| c == "juego"));
        if cab_ok { rd.records().filter(Result::is_ok).count() } else { 0 }
    });
    let json_ok = leer_texto(centro).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).is_some_and(|v| v.get("filas").is_some());
    filas.unwrap_or(0) >= 1000 && json_ok
}

/// Solo una comprobación a la vez: la de cada seis horas y la del botón «Buscar actualizaciones» pueden
/// coincidir, y las dos escriben en la misma carpeta.
static CERROJO: Mutex<()> = Mutex::new(());

/// Mira en GitHub si hay un catálogo distinto del que se usa y, si lo hay, lo descarga. Devuelve true si
/// ha cambiado el catálogo que se usa (la ventana tiene que volver a cargarlo).
pub fn actualizar(r: &Rutas) -> Result<bool, String> {
    let _solo_uno = CERROJO.lock().unwrap_or_else(|e| e.into_inner());
    let remoto = descarga::carpeta_repositorio(RUTA_REPOSITORIO)?;
    let (Some(rc), Some(rj)) = (remoto.get(CSV), remoto.get(CENTRO)) else {
        return Err("El repositorio no tiene el catálogo.".into());
    };
    let antes = activo(r);
    let d = carpeta(r);
    // El que lleva dentro el programa ya es el último: lo descargado sobra.
    if sha_git(&r.csv).as_deref() == Some(&rc.0) && sha_git(&r.centro).as_deref() == Some(&rj.0) {
        if d.exists() {
            let _ = fs::remove_dir_all(&d);
        }
        return Ok(antes != activo(r));
    }
    // Ya está descargado.
    if let Some(e) = estado(r) {
        if e.programa == PROGRAMA && e.sha.get(CSV) == Some(&rc.0) && e.sha.get(CENTRO) == Some(&rj.0) && d.join(CSV).is_file() && d.join(CENTRO).is_file() {
            return Ok(false);
        }
    }
    let nuevo = d.with_extension("nuevo");
    let _ = fs::remove_dir_all(&nuevo);
    fs::create_dir_all(&nuevo).map_err(|e| e.to_string())?;
    let res = (|| -> Result<(), String> {
        for (nombre, (sha, url)) in [(CSV, rc), (CENTRO, rj)] {
            let f = nuevo.join(nombre);
            descarga::bajar_a(url, &f)?;
            if sha_git(&f).as_deref() != Some(sha.as_str()) {
                return Err("El catálogo descargado no está entero: si acaba de cambiar, inténtalo en unos minutos.".into());
            }
        }
        if !valido(&nuevo.join(CSV), &nuevo.join(CENTRO)) {
            return Err("El catálogo descargado no es válido.".into());
        }
        let e = Estado { programa: PROGRAMA.into(), sha: HashMap::from([(CSV.into(), rc.0.clone()), (CENTRO.into(), rj.0.clone())]) };
        fs::write(nuevo.join("estado.json"), serde_json::to_vec_pretty(&e).unwrap_or_default()).map_err(|e| e.to_string())?;
        let _ = fs::remove_dir_all(&d);
        fs::rename(&nuevo, &d).map_err(|e| e.to_string())
    })();
    if res.is_err() {
        let _ = fs::remove_dir_all(&nuevo);
    }
    res.map(|_| true)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn sha_como_git() {
        let d = crate::motor::pruebas::carpeta("catalogo-sha");
        let f = d.join("hola.txt");
        fs::write(&f, "hello\n").unwrap();
        // `git hash-object` de "hello\n".
        assert_eq!(sha_git(&f).unwrap(), "ce013625030ba8dba906f756967f9e9ca394464a");
    }

    /// Descarga de verdad el catálogo de GitHub en una carpeta de datos de prueba (necesita conexión):
    /// cargo test --lib -- --ignored
    #[test]
    #[ignore]
    fn descarga_el_de_github() {
        let d = crate::motor::pruebas::carpeta("catalogo-github");
        let recursos = d.join("recursos");
        fs::create_dir_all(recursos.join("catalogo")).unwrap();
        // Un catálogo «de dentro» distinto del de GitHub, para que tenga que descargarlo.
        fs::write(recursos.join("catalogo").join(CSV), "sistema,juego\n").unwrap();
        fs::write(recursos.join("catalogo").join(CENTRO), "{}").unwrap();
        let r = Rutas::de(&recursos, &d.join("datos"));
        assert!(actualizar(&r).unwrap());
        let (c, j) = activo(&r);
        assert!(c.starts_with(d.join("datos")) && valido(&c, &j));
        // La segunda vez ya lo tiene.
        assert!(!actualizar(&r).unwrap());
    }
}
