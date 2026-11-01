//! Biblioteca de traducciones: la ventana (ui/) pide aquí el catálogo, tus juegos y el estado de los
//! trabajos, y manda buscar, traducir, jugar o abrir páginas. El catálogo va dentro del programa y el
//! parcheo lo hace él mismo (motor.rs, parcheo.rs y descarga.rs, con 7-Zip, chdman y xdelta3); tus juegos
//! están en las carpetas que elijas (C:\Juegos si no dices otra).

mod catalogo;
mod datos;
mod descarga;
mod escaneo;
mod jugar;
mod listas;
mod motor;
mod parcheo;
mod portadas;
mod rutas;
mod trabajos;

use rutas::{Ajustes, Rutas};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

pub struct App {
    rutas: Rutas,
    ajustes: Mutex<Ajustes>,
    /// Primera vez que se abre (sin ajustes guardados).
    nuevo: AtomicBool,
    config: PathBuf,
    datos: Mutex<datos::Datos>,
    cola: trabajos::Cola,
    partida: Mutex<Option<Child>>,
}

type Estado<'a> = State<'a, Arc<App>>;

impl App {
    fn ajustes(&self) -> Ajustes {
        self.ajustes.lock().unwrap().clone()
    }

    /// Lee el catálogo si hace falta.
    fn con_datos<T>(&self, f: impl FnOnce(&datos::Datos) -> T) -> Result<T, String> {
        let mut d = self.datos.lock().unwrap();
        d.cargar(&self.rutas)?;
        Ok(f(&d))
    }

    /// El juego tiene que estar en una de tus carpetas de juegos.
    fn juego_valido(&self, juego: &Path) -> bool {
        let a = self.ajustes();
        juego.is_file() && a.carpetas.iter().any(|c| Rutas::dentro(juego, c))
    }
}

// ---------------------------------------------------------------- órdenes de la ventana

#[tauri::command]
fn datos(app: Estado) -> Result<Response, String> {
    app.con_datos(|d| Response::new(d.json(&app.rutas)))
}

#[tauri::command]
fn mis_juegos(app: Estado) -> Response {
    Response::new(datos::leer_texto(&app.rutas.mis_juegos).unwrap_or_else(|_| r#"{"fecha":null,"juegos":[],"tes":[]}"#.into()))
}

#[tauri::command]
fn estado(app: Estado) -> Value {
    let (escaneando, progreso) = {
        let g = app.cola.m.lock().unwrap();
        (g.escaneando, g.progreso.clone())
    };
    json!({
        "trabajos": trabajos::lista(&app.rutas),
        "escaneo": {"activo": escaneando, "progreso": progreso},
        "espera": trabajos::estado_espera(&app.cola),
    })
}

#[tauri::command]
fn escanear(app: Estado) {
    app.cola.escanear();
}

#[tauri::command]
fn traducir(h: AppHandle, app: Estado, clave: String, juego: PathBuf) -> Result<String, String> {
    if !app.juego_valido(&juego) {
        return Err("El juego tiene que estar en una de tus carpetas de juegos (Ajustes).".into());
    }
    let id = app.con_datos(|d| trabajos::nuevo(&app.rutas, d, &app.cola, &clave, &juego, None))??;
    // La ventana empieza a mirar el progreso en cuanto el trabajo entra en la cola.
    let _ = h.emit("trabajo", ());
    Ok(id)
}

#[tauri::command]
fn esperar(app: Estado, clave: String, juego: PathBuf) -> Result<String, String> {
    if !app.juego_valido(&juego) {
        return Err("El juego tiene que estar en una de tus carpetas de juegos (Ajustes).".into());
    }
    Ok(trabajos::esperar(&app.cola, &clave, &juego).display().to_string())
}

#[tauri::command]
fn cancelar_espera(app: Estado) {
    app.cola.m.lock().unwrap().espera = None;
}

#[tauri::command]
fn jugar(app: Estado, ruta: PathBuf, sistema: String) -> Result<String, String> {
    let a = app.ajustes();
    jugar::abrir(a.retrobat.as_deref(), &ruta, &sistema, &mut app.partida.lock().unwrap())
}

#[tauri::command]
fn ver_carpeta(ruta: PathBuf) -> Result<(), String> {
    if !ruta.exists() {
        return Err("Esa carpeta ya no existe.".into());
    }
    tauri_plugin_opener::reveal_item_in_dir(&ruta).map_err(|e| e.to_string())
}

#[tauri::command]
fn abrir_web(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Solo se abren páginas web.".into());
    }
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn ajustes(app: Estado) -> Value {
    let a = app.ajustes();
    json!({
        "carpetas": a.carpetas, "salida": a.salida, "retrobat": a.retrobat, "tiendas": a.tiendas,
        "descargas": trabajos::carpeta_descargas(), "datos": app.rutas.datos,
        "version": env!("CARGO_PKG_VERSION"), "nuevo": app.nuevo.load(Ordering::Relaxed),
        "sugeridas": a.sugeridas().into_iter().map(|p| json!({"ruta": p, "existe": p.is_dir()})).collect::<Vec<_>>(),
    })
}

#[tauri::command]
fn guardar_ajustes(app: Estado, carpetas: Vec<PathBuf>, salida: String, retrobat: Option<PathBuf>, tiendas: bool) -> Result<Value, String> {
    let carpetas: Vec<PathBuf> = carpetas.into_iter().filter(|c| !c.as_os_str().is_empty()).collect();
    if carpetas.is_empty() {
        return Err("Hace falta al menos una carpeta de juegos.".into());
    }
    if let Some(r) = retrobat.as_ref().filter(|r| !r.as_os_str().is_empty()) {
        if !r.join("emulationstation").join("emulatorLauncher.exe").is_file() {
            return Err(format!("En {} no está emulationstation\\emulatorLauncher.exe de RetroBat.", r.display()));
        }
    }
    // Las carpetas que has elegido y aún no existen (C:\Juegos, por ejemplo) se crean ahora.
    for c in &carpetas {
        fs::create_dir_all(c).map_err(|e| format!("No se pudo crear {}: {e}", c.display()))?;
    }
    let a = Ajustes { carpetas, salida, retrobat: retrobat.filter(|r| !r.as_os_str().is_empty()), tiendas };
    a.guardar(&app.config).map_err(|e| e.to_string())?;
    *app.ajustes.lock().unwrap() = a;
    app.nuevo.store(false, Ordering::Relaxed);
    app.cola.escanear();
    Ok(ajustes(app))
}

#[tauri::command]
async fn elegir_carpeta(h: AppHandle, titulo: String) -> Option<PathBuf> {
    h.dialog().file().set_title(titulo).blocking_pick_folder().and_then(|p| p.into_path().ok())
}

#[tauri::command]
async fn elegir_imagen(h: AppHandle) -> Option<PathBuf> {
    h.dialog()
        .file()
        .set_title("Elige la portada")
        .add_filter("Imágenes", &["png", "jpg", "jpeg", "webp", "gif", "bmp", "avif"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
}

#[tauri::command]
fn portadas_propias(app: Estado) -> Value {
    portadas::leer(&app.rutas)
}

#[tauri::command]
fn poner_portada(app: Estado, clave: String, origen: String) -> Result<Value, String> {
    portadas::poner(&app.rutas, &clave, &origen)
}

#[tauri::command]
fn mis_listas(app: Estado) -> Value {
    listas::leer(&app.rutas)
}

#[tauri::command]
fn guardar_listas(app: Estado, listas: Value) -> Result<Value, String> {
    listas::guardar(&app.rutas, &listas)
}

#[tauri::command]
fn quitar_portada(app: Estado, clave: String) -> Result<Value, String> {
    portadas::quitar(&app.rutas, &clave)
}

/// ¿Hay una versión más nueva publicada en GitHub? Se mira una vez al abrir; sin conexión, no dice nada.
#[tauri::command]
async fn version_nueva() -> Value {
    tauri::async_runtime::spawn_blocking(|| descarga::version_nueva(env!("CARGO_PKG_VERSION"))).await.ok().flatten().unwrap_or(Value::Null)
}

/// Una imagen pegada con Ctrl+V: llega tal cual (sin pasar a JSON) y la clave del juego, en una cabecera.
#[tauri::command]
fn pegar_portada(app: Estado, request: tauri::ipc::Request<'_>) -> Result<Value, String> {
    let tauri::ipc::InvokeBody::Raw(datos) = request.body() else {
        return Err("No ha llegado ninguna imagen.".into());
    };
    let clave = request.headers().get("clave").and_then(|v| v.to_str().ok()).map(portadas::descodificar).ok_or("Falta el juego.")?;
    portadas::pegar(&app.rutas, &clave, datos)
}

// ---------------------------------------------------------------- hilos de fondo

/// Ejecuta la cola de una en una y avisa a la ventana al acabar cada tarea.
fn trabajador(h: AppHandle, app: Arc<App>) {
    std::thread::spawn(move || loop {
        let t = app.cola.siguiente();
        let evento = trabajos::ejecutar(&app.rutas, &app.ajustes(), &app.cola, t);
        let _ = h.emit(evento, ());
    });
}

/// Mira la carpeta Descargas cada dos segundos mientras se espera un parche.
fn vigilante(h: AppHandle, app: Arc<App>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        if let Some((clave, juego, parche)) = trabajos::revisar_espera(&app.rutas, &app.cola) {
            let _ = app.con_datos(|d| trabajos::nuevo(&app.rutas, d, &app.cola, &clave, &juego, Some(&parche)));
            let _ = h.emit("trabajo", ());
        }
    });
}

/// Al poco de abrir, mira si hay un catálogo más nuevo en GitHub; si lo descarga, la ventana lo recarga.
fn catalogo_al_dia(h: AppHandle, app: Arc<App>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(5));
        if let Ok(true) = catalogo::actualizar(&app.rutas) {
            let _ = h.emit("catalogo", ());
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            let rutas = Rutas::de(&app.path().resource_dir()?, &app.path().app_local_data_dir()?);
            rutas.crear_carpetas();
            let (ajustes, nuevo) = Ajustes::cargar(&config);
            let estado = Arc::new(App {
                rutas,
                ajustes: Mutex::new(ajustes),
                nuevo: AtomicBool::new(nuevo),
                config,
                datos: Mutex::new(datos::Datos::default()),
                cola: trabajos::Cola::default(),
                partida: Mutex::new(None),
            });
            // Busca tus juegos al abrir (los CRC ya calculados se reutilizan, así que es rápido).
            estado.cola.escanear();
            trabajador(app.handle().clone(), estado.clone());
            vigilante(app.handle().clone(), estado.clone());
            catalogo_al_dia(app.handle().clone(), estado.clone());
            app.manage(estado);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            datos, mis_juegos, estado, escanear, traducir, esperar, cancelar_espera, jugar, ver_carpeta, abrir_web, ajustes,
            guardar_ajustes, elegir_carpeta, elegir_imagen, portadas_propias, poner_portada, quitar_portada, pegar_portada, version_nueva,
            mis_listas, guardar_listas
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo abrir la Biblioteca de traducciones");
}
