//! Cuánta gente usa el programa. Al abrirlo, y como mucho una vez al día, avisa a las estadísticas de la web
//! (GoatCounter, las mismas que cuentan las visitas de bibliotecadetraducciones.com) de que se ha usado y con
//! qué versión. No manda nada más: ni tus juegos, ni tus carpetas, ni un identificador del equipo; GoatCounter
//! no usa cookies ni guarda la IP. Se quita en Ajustes («Estadísticas de uso»). En el panel de GoatCounter sale
//! como el evento «app/<versión>»: sus visitantes de cada día son las personas que lo han abierto ese día.

use crate::rutas::Rutas;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DESTINO: &str = "https://stats.bibliotecadetraducciones.com/count";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Con un espacio y una barra: GoatCounter toma por robots, y no los cuenta, los User-Agent sin ellos (el
/// «Biblioteca-de-traducciones/<versión>» de las descargas, por ejemplo).
const AGENTE: &str = concat!("Biblioteca-de-traducciones/", env!("CARGO_PKG_VERSION"), " (Windows)");
const FICHERO: &str = "uso.txt";

/// El día de hoy (en días desde 1970, hora universal).
fn hoy() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400)
}

/// ¿Ya se avisó ese día? El fichero guarda el día del último aviso.
fn avisado(f: &Path, dia: u64) -> bool {
    fs::read_to_string(f).ok().and_then(|t| t.trim().parse::<u64>().ok()) == Some(dia)
}

/// La dirección del aviso: el evento «app/<versión>» de GoatCounter.
fn direccion() -> String {
    let datos = [("p", format!("app/{VERSION}")), ("t", format!("Programa abierto ({VERSION})")), ("e", "true".into())];
    url::Url::parse_with_params(DESTINO, &datos).map(String::from).unwrap_or_default()
}

/// Avisa de que se ha usado hoy, si no se ha hecho ya. Sin conexión no pasa nada: se intenta la próxima vez.
pub fn avisar(r: &Rutas) {
    let (f, dia) = (r.datos.join(FICHERO), hoy());
    if avisado(&f, dia) {
        return;
    }
    let Ok(tls) = native_tls::TlsConnector::new() else { return };
    let agente = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(20))
        .user_agent(AGENTE)
        .tls_connector(Arc::new(tls))
        .build();
    if agente.get(&direccion()).call().is_ok_and(|res| (200..300).contains(&res.status())) {
        let _ = fs::write(&f, dia.to_string());
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn solo_la_version() {
        let d = direccion();
        assert!(d.starts_with("https://stats.bibliotecadetraducciones.com/count?"));
        assert!(d.contains(&format!("p=app%2F{VERSION}&")) && d.ends_with("&e=true"));
        assert!(d.contains("t=Programa+abierto+%28"));
    }

    /// Las reglas de GoatCounter para no contar robots (isbot: UserAgent).
    #[test]
    fn goatcounter_no_lo_toma_por_robot() {
        assert!(AGENTE.len() >= 10 && AGENTE.contains(' ') && AGENTE.contains('/') && !AGENTE.contains("://"));
        let a = AGENTE.to_lowercase();
        assert!(!["bot", "crawler", "spider", "ruby", "java/", "curl/", "wget/", "httpclient/"].iter().any(|x| a.contains(x)));
    }

    #[test]
    fn una_vez_al_dia() {
        let f = crate::motor::pruebas::carpeta("uso").join(FICHERO);
        assert!(!avisado(&f, 20_401));
        fs::write(&f, "20401").unwrap();
        assert!(avisado(&f, 20_401));
        assert!(!avisado(&f, 20_402));
    }
}
