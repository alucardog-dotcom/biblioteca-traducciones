//! Dónde está todo. El programa lleva dentro el catálogo y las herramientas de parcheo (7z, chdman y
//! xdelta3, en la carpeta de recursos), guarda su trabajo en su propia carpeta de datos y busca tus juegos
//! en las carpetas que elijas (C:\Juegos si no dices otra). RetroBat es opcional: solo se usa para Jugar.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const CARPETA_POR_DEFECTO: &str = r"C:\Juegos";

#[derive(Debug, Clone)]
pub struct Rutas {
    pub csv: PathBuf,
    pub centro: PathBuf,
    pub siete_z: PathBuf,
    pub chdman: PathBuf,
    pub xdelta: PathBuf,
    pub datos: PathBuf,
    pub trabajos: PathBuf,
    pub parches: PathBuf,
    pub mis_juegos: PathBuf,
    pub traducidos: PathBuf,
    pub cache_crc: PathBuf,
    /// Lo ya aplicado (parche y juego -> traducido), para no repetirlo.
    pub hechos: PathBuf,
    /// Paquetes descargados de los que solo se saca el parche de la ficha.
    pub descargas: PathBuf,
    pub temp: PathBuf,
}

/// Quita el prefijo `\\?\` que pone Windows a algunas rutas: 7z y chdman no lo entienden.
pub fn normal(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(r) if r.as_bytes().get(1) == Some(&b':') => PathBuf::from(r),
        _ => p.to_path_buf(),
    }
}

impl Rutas {
    pub fn de(recursos: &Path, datos: &Path) -> Rutas {
        let (recursos, datos) = (normal(recursos), normal(datos));
        let (recursos, datos) = (recursos.as_path(), datos.as_path());
        let cat = recursos.join("catalogo");
        let her = recursos.join("herramientas");
        Rutas {
            csv: cat.join("traducciones_fan.csv"),
            centro: cat.join("centro_datos.json"),
            siete_z: her.join("7z.exe"),
            chdman: her.join("chdman.exe"),
            xdelta: her.join("xdelta3.exe"),
            trabajos: datos.join("trabajos"),
            parches: datos.join("parches"),
            mis_juegos: datos.join("mis_juegos.json"),
            traducidos: datos.join("traducidos.tsv"),
            cache_crc: datos.join("crc_biblioteca.json"),
            hechos: datos.join("hechos.tsv"),
            descargas: datos.join("descargas"),
            temp: datos.join("_temp"),
            datos: datos.to_path_buf(),
        }
    }

    pub fn crear_carpetas(&self) {
        for d in [&self.trabajos, &self.parches] {
            let _ = fs::create_dir_all(d);
        }
    }

    /// Al arrancar, antes de que empiece ningún trabajo: fuera los restos de `_temp` (si el programa se
    /// cerró a mitad de un parcheo pueden ser gigabytes) y los trabajos de hace más de 30 días.
    pub fn limpiar_al_arrancar(&self) {
        let _ = fs::remove_dir_all(&self.temp);
        let limite = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 24 * 3600);
        for e in fs::read_dir(&self.trabajos).into_iter().flatten().flatten() {
            if e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < limite) {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    /// `p` está dentro de `raiz` (sin `..` que se salgan).
    pub fn dentro(p: &Path, raiz: &Path) -> bool {
        match (fs::canonicalize(p), fs::canonicalize(raiz)) {
            (Ok(a), Ok(b)) => a.starts_with(b),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ajustes {
    /// Carpetas donde están tus juegos (se miran también sus subcarpetas).
    #[serde(default = "carpetas_por_defecto")]
    pub carpetas: Vec<PathBuf>,
    /// "junto": el traducido va al lado del original. Si es una carpeta, va a <carpeta>\<sistema>.
    #[serde(default = "junto")]
    pub salida: String,
    /// Carpeta de RetroBat, si lo tienes: el botón Jugar usa su lanzador.
    #[serde(default)]
    pub retrobat: Option<PathBuf>,
    /// Buscar también los juegos de PC instalados con Steam, GOG y Epic.
    #[serde(default = "si")]
    pub tiendas: bool,
}

fn carpetas_por_defecto() -> Vec<PathBuf> {
    vec![PathBuf::from(CARPETA_POR_DEFECTO)]
}
fn junto() -> String {
    "junto".into()
}
fn si() -> bool {
    true
}

impl Default for Ajustes {
    fn default() -> Self {
        let retrobat = PathBuf::from(r"C:\RetroBat");
        Ajustes {
            carpetas: carpetas_por_defecto(),
            salida: junto(),
            retrobat: retrobat.join("emulationstation").join("emulatorLauncher.exe").is_file().then_some(retrobat),
            tiendas: true,
        }
    }
}

impl Ajustes {
    /// Los ajustes guardados y si es la primera vez (no hay ninguno guardado: la ventana da la
    /// bienvenida y pregunta dónde están tus juegos; no se crea ninguna carpeta sin preguntar).
    pub fn cargar(dir: &Path) -> (Ajustes, bool) {
        let a: Option<Ajustes> = fs::read(dir.join("ajustes.json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let nuevo = a.is_none();
        (a.unwrap_or_default(), nuevo)
    }

    /// Carpetas que propone la bienvenida: la de ROMs de RetroBat si lo tienes, y C:\Juegos.
    pub fn sugeridas(&self) -> Vec<PathBuf> {
        let mut s: Vec<PathBuf> = self.retrobat.iter().map(|r| r.join("roms")).filter(|r| r.is_dir()).collect();
        s.push(PathBuf::from(CARPETA_POR_DEFECTO));
        s
    }

    pub fn guardar(&self, dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        crate::datos::escribir_atomico(&dir.join("ajustes.json"), &serde_json::to_vec_pretty(self).unwrap_or_default())
    }

}
