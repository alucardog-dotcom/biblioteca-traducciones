//! La cola de tareas: buscar tus juegos y traducir, de una en una. Buscar lo hace escaneo.rs; traducir,
//! descarga.rs (bajar el parche) y parcheo.rs (aplicarlo). Cada trabajo deja su estado en
//! <datos>\trabajos\<id>.estado.json y lo que ha hecho en <id>.log.
//!
//! También vigila la carpeta Descargas cuando una traducción no tiene descarga directa: el parche
//! que bajes de su página se aplica solo.

use crate::datos::{self, Datos};
use crate::descarga;
use crate::escaneo::{self, Escaner};
use crate::parcheo::{self, Entorno};
use crate::rutas::{Ajustes, Rutas};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const EXT_PARCHE: [&str; 10] = ["zip", "7z", "rar", "ips", "bps", "ups", "ppf", "xdelta", "vcdiff", "aps"];
const ESPERA_MAX: Duration = Duration::from_secs(30 * 60);

pub enum Tarea {
    Escanear,
    Traducir { id: String, spec: PathBuf },
}

pub struct Espera {
    pub clave: String,
    pub juego: PathBuf,
    pub dir: PathBuf,
    pub hasta: SystemTime,
    conocidos: HashSet<(PathBuf, u64)>,
    vistos: HashMap<PathBuf, u64>,
}

#[derive(Default)]
pub struct Interior {
    pub pendientes: VecDeque<Tarea>,
    pub escaneando: bool,
    pub progreso: String,
    pub espera: Option<Espera>,
}

#[derive(Default)]
pub struct Cola {
    pub m: Mutex<Interior>,
    pub cv: Condvar,
}

impl Cola {
    pub fn escanear(&self) {
        let mut g = self.m.lock().unwrap();
        if !g.escaneando {
            g.escaneando = true;
            g.pendientes.push_back(Tarea::Escanear);
            self.cv.notify_one();
        }
    }

    pub fn siguiente(&self) -> Tarea {
        let mut g = self.m.lock().unwrap();
        loop {
            if let Some(t) = g.pendientes.pop_front() {
                return t;
            }
            g = self.cv.wait(g).unwrap();
        }
    }

    fn progreso(&self, p: String) {
        self.m.lock().unwrap().progreso = p;
    }
}

fn ahora() -> String {
    // Segundos desde 1970: la página solo los compara.
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).to_string()
}

fn escribir_estado(r: &Rutas, id: &str, clave: &str, estado: &str, mensaje: &str, fin: &str, log: &[String]) {
    let o = json!({"id": id, "clave": clave, "estado": estado, "mensaje": mensaje, "final": fin, "log": log, "actualizado": ahora()});
    let _ = fs::write(r.trabajos.join(format!("{id}.estado.json")), serde_json::to_vec_pretty(&o).unwrap_or_default());
}

/// Un trabajo en marcha: su estado para la ventana y lo que va haciendo.
struct Trabajo<'a> {
    r: &'a Rutas,
    id: String,
    clave: String,
    log: Vec<String>,
}

impl Trabajo<'_> {
    fn estado(&self, estado: &str, mensaje: &str, fin: &str) {
        escribir_estado(self.r, &self.id, &self.clave, estado, mensaje, fin, &self.log);
    }
}

/// Los CRC32 de la versión que pide la traducción: los de la descarga (la de RetroAchievements a veces
/// es otra revisión) y los de la ficha.
fn crc_base(d: Option<&Value>, ficha: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let de_descarga = d.and_then(|d| d["c"].as_array()).into_iter().flatten().filter_map(Value::as_str);
    for c in de_descarga.chain(ficha.split('|')).map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()) {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Descarga la traducción de la ficha y la aplica a tu juego. Si la de una descarga no vale para tu
/// copia, se prueba con la siguiente.
fn traducir(r: &Rutas, a: &Ajustes, spec: &Value, t: &mut Trabajo) -> Result<PathBuf, String> {
    let sis = spec["sistema"].as_str().unwrap_or("");
    let juego = PathBuf::from(spec["juego"].as_str().unwrap_or(""));
    let entorno = Entorno { siete_z: &r.siete_z, chdman: &r.chdman, xdelta: &r.xdelta, temp: &r.temp, hechos: &r.hechos, salida: &a.salida };
    let destino = r.parches.join(sis);
    let local = spec["local"].as_str().filter(|l| !l.is_empty());
    let fuentes: Vec<Option<&Value>> = match local {
        Some(_) => vec![None],
        None => spec["descargas"].as_array().into_iter().flatten().map(Some).collect(),
    };
    let (mut descargado, mut motivo) = (false, None);
    for d in fuentes {
        let mut archivos = vec![];
        if let Some(l) = local {
            archivos.push(PathBuf::from(l));
        } else if let Some(d) = d {
            t.estado("descargando", "Descargando la traducción…", "");
            let urls = if d["t"] == "github" {
                match descarga::urls_github(d["r"].as_str().unwrap_or(""), d["tag"].as_str().unwrap_or("")) {
                    Ok(u) => u,
                    Err(m) => {
                        t.log.push(m);
                        continue;
                    }
                }
            } else {
                match &d["u"] {
                    Value::String(u) => vec![u.clone()],
                    v => v.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect(),
                }
            };
            let elegir: Vec<String> = d["f"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
            for u in urls {
                let res = if elegir.is_empty() {
                    descarga::descargar(&u, &destino, &mut t.log).map(|f| vec![f])
                } else {
                    descarga::descargar(&u, &r.descargas, &mut t.log).and_then(|paquete| {
                        let sacados = descarga::elegidos(&r.siete_z, &paquete, &elegir, &destino, &r.temp);
                        let _ = fs::remove_file(&paquete);
                        if sacados.as_ref().is_ok_and(Vec::is_empty) {
                            t.log.push(format!("{} no trae el parche de esta ficha.", paquete.file_name().unwrap_or_default().to_string_lossy()));
                        }
                        sacados
                    })
                };
                match res {
                    Ok(fs_) => archivos.extend(fs_),
                    Err(m) => t.log.push(format!("No se pudo descargar {}: {m}", descarga::nombre_seguro(&u))),
                }
            }
        }
        if archivos.is_empty() {
            continue;
        }
        descargado = true;
        t.estado("parcheando", "Aplicando la traducción a tu juego…", "");
        let crc = crc_base(d, spec["crc"].as_str().unwrap_or(""));
        for arch in archivos {
            match parcheo::traducir(&entorno, sis, &arch, &juego, &crc, &mut t.log) {
                Ok(fin) => return Ok(fin),
                Err(m) => motivo = Some(m),
            }
        }
    }
    if !descargado {
        return Err("No se pudo descargar la traducción. Prueba con «Abrir página y esperar la descarga».".into());
    }
    Err(motivo.unwrap_or_else(|| "No se pudo aplicar la traducción.".into()))
}

fn leer_json(p: &Path) -> Option<Value> {
    datos::leer_texto(p).ok().and_then(|t| serde_json::from_str(&t).ok())
}

static CONTADOR: AtomicU32 = AtomicU32::new(0);

/// Prepara el trabajo (qué traducción, a qué juego y de dónde bajarla) y lo pone en la cola.
pub fn nuevo(r: &Rutas, d: &Datos, cola: &Cola, clave: &str, juego: &Path, local: Option<&Path>) -> Result<String, String> {
    let fila = d.filas.get(clave).ok_or("No encuentro esa traducción en el catálogo.")?;
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|x| x.as_millis()).unwrap_or(0);
    let id = format!("{ms}-{:04x}", CONTADOR.fetch_add(1, Ordering::Relaxed) & 0xffff);
    let extra = d.extra(clave);
    let descargas = extra.and_then(|x| x.get("d")).cloned().unwrap_or_else(|| json!([]));
    let spec = json!({
        "id": id, "clave": clave, "sistema": fila[datos::SISTEMA], "juego": juego, "crc": fila[datos::CRC],
        "titulo": fila[datos::JUEGO], "grupo": fila[datos::GRUPO], "version": fila[datos::VERSION],
        "descargas": descargas, "portada": Value::Null, "local": local,
    });
    let path = r.trabajos.join(format!("{id}.json"));
    fs::write(&path, serde_json::to_vec_pretty(&spec).unwrap_or_default()).map_err(|e| e.to_string())?;
    escribir_estado(r, &id, clave, "en-cola", "En espera…", "", &[]);
    let mut g = cola.m.lock().unwrap();
    g.pendientes.push_back(Tarea::Traducir { id: id.clone(), spec: path });
    cola.cv.notify_one();
    Ok(id)
}

/// Busca tus juegos en tus carpetas y en las tiendas de PC, y deja la lista en mis_juegos.json.
fn escanear(r: &Rutas, a: &Ajustes, cola: &Cola) {
    let mut juegos = Vec::new();
    let mut tes = Vec::new();
    let mut esc = Escaner::new(&r.cache_crc, &r.siete_z);
    let mut carpetas = a.carpetas.clone();
    // Lo traducido a una carpeta aparte también se busca ahí.
    if a.salida != "junto" && !a.salida.trim().is_empty() {
        carpetas.push(PathBuf::from(&a.salida));
    }
    for c in &carpetas {
        cola.progreso(format!("Buscando en {}", c.display()));
        esc.carpeta(c, &mut juegos, &mut tes);
    }
    esc.guardar(&r.cache_crc);
    if a.tiendas {
        cola.progreso("Buscando juegos de Steam, GOG y Epic".into());
        escaneo::tiendas(&mut juegos);
    }
    let o = json!({"fecha": ahora(), "juegos": juegos, "tes": tes});
    let _ = fs::write(&r.mis_juegos, serde_json::to_vec(&o).unwrap_or_default());
}

/// Ejecuta una tarea (bloquea hasta que termina). Devuelve el nombre del evento para la ventana.
pub fn ejecutar(r: &Rutas, a: &Ajustes, cola: &Cola, t: Tarea) -> &'static str {
    match t {
        Tarea::Escanear => {
            escanear(r, a, cola);
            let mut g = cola.m.lock().unwrap();
            g.escaneando = false;
            g.progreso.clear();
            "juegos"
        }
        Tarea::Traducir { id, spec } => {
            let spec = leer_json(&spec).unwrap_or(Value::Null);
            let clave = spec["clave"].as_str().unwrap_or("").to_string();
            let mut t = Trabajo { r, id: id.clone(), clave: clave.clone(), log: vec![] };
            // Un fallo inesperado no puede dejar el trabajo «en marcha» para siempre ni parar la cola.
            match catch_unwind(AssertUnwindSafe(|| traducir(r, a, &spec, &mut t))) {
                Ok(Ok(fin)) => {
                    let fin = fin.display().to_string();
                    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&r.traducidos) {
                        let _ = writeln!(f, "{clave}\t{fin}\t{}", ahora());
                    }
                    t.estado("hecho", "Traducido.", &fin);
                }
                Ok(Err(m)) => {
                    if !t.log.last().is_some_and(|l| l.contains(&m)) {
                        t.log.push(m.clone());
                    }
                    t.estado("error", &m, "");
                }
                Err(_) => t.estado("error", "Algo falló al traducir. Mira el registro del trabajo.", ""),
            }
            let _ = fs::write(r.trabajos.join(format!("{id}.log")), t.log.join("\r\n"));
            "trabajo"
        }
    }
}

/// Trabajos de las últimas 12 horas, del más antiguo al más nuevo.
pub fn lista(r: &Rutas) -> Vec<Value> {
    let limite = SystemTime::now() - Duration::from_secs(12 * 3600);
    let mut fs_: Vec<PathBuf> = fs::read_dir(&r.trabajos)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| p.to_string_lossy().ends_with(".estado.json"))
                .filter(|p| fs::metadata(p).and_then(|m| m.modified()).is_ok_and(|t| t > limite))
                .collect()
        })
        .unwrap_or_default();
    fs_.sort();
    fs_.iter().filter_map(|p| leer_json(p)).collect()
}

// ---------------------------------------------------------------- espera de descargas manuales

pub fn carpeta_descargas() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads"))
}

fn parches_en(dir: &Path) -> Vec<(PathBuf, u64)> {
    fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    EXT_PARCHE.iter().any(|x| n.ends_with(&format!(".{x}")))
                })
                .map(|e| (e.path(), e.metadata().map(|m| m.len()).unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default()
}

pub fn esperar(cola: &Cola, clave: &str, juego: &Path) -> PathBuf {
    let dir = carpeta_descargas();
    let conocidos = parches_en(&dir).into_iter().collect();
    cola.m.lock().unwrap().espera = Some(Espera {
        clave: clave.to_string(),
        juego: juego.to_path_buf(),
        dir: dir.clone(),
        hasta: SystemTime::now() + ESPERA_MAX,
        conocidos,
        vistos: HashMap::new(),
    });
    dir
}

/// Una pasada de la vigilancia: si ha llegado un parche nuevo y ya no crece, se copia a
/// <datos>\parches\<sistema> y se devuelve (clave, juego, parche) para crear el trabajo.
pub fn revisar_espera(r: &Rutas, cola: &Cola) -> Option<(String, PathBuf, PathBuf)> {
    let mut g = cola.m.lock().unwrap();
    let e = g.espera.as_mut()?;
    if SystemTime::now() > e.hasta {
        g.espera = None;
        return None;
    }
    for (p, len) in parches_en(&e.dir) {
        if e.conocidos.contains(&(p.clone(), len)) {
            continue;
        }
        // Se espera a que el tamaño deje de cambiar (descarga terminada).
        if e.vistos.get(&p) != Some(&len) {
            e.vistos.insert(p, len);
            continue;
        }
        let sis = e.clave.split('|').next().unwrap_or("").to_string();
        let dest_dir = r.parches.join(&sis);
        let _ = fs::create_dir_all(&dest_dir);
        let dest = dest_dir.join(p.file_name()?);
        if fs::copy(&p, &dest).is_err() {
            continue;
        }
        let res = (e.clave.clone(), e.juego.clone(), dest);
        g.espera = None;
        return Some(res);
    }
    None
}

pub fn estado_espera(cola: &Cola) -> Value {
    let g = cola.m.lock().unwrap();
    match &g.espera {
        Some(e) => json!({"clave": e.clave, "dir": e.dir}),
        None => Value::Null,
    }
}
