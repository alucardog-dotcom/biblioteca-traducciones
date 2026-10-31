//! Bajar el parche de una traducción: lo que hacía TrabajoTraducir.ps1, ya sin PowerShell.
//!
//! Solo se descarga de los sitios de la lista (Traducciones Wave, archive.org y GitHub). Las redirecciones
//! se siguen a mano para comprobar que todas van a sitios permitidos, y una página web en lugar del fichero
//! (un enlace caído, un aviso) no se toma por un parche.

use crate::parcheo::{descomprimir, mover, Temporal};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use url::Url;

/// También sus subdominios (los servidores ia800… de archive.org, objects.githubusercontent.com…).
const PERMITIDOS: [&str; 4] = ["traduccioneswave.top", "archive.org", "github.com", "githubusercontent.com"];
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const AGENTE: &str = concat!("Biblioteca-de-traducciones/", env!("CARGO_PKG_VERSION"));

fn agente() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("No se pudo preparar la conexión segura: {e}"))?;
    Ok(ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(Duration::from_secs(60))
        .timeout_read(Duration::from_secs(300))
        .user_agent(AGENTE)
        .tls_connector(Arc::new(tls))
        .build())
}

fn permitido(u: &Url) -> bool {
    let h = u.host_str().unwrap_or("").to_lowercase();
    u.scheme() == "https" && PERMITIDOS.iter().any(|p| h == *p || h.ends_with(&format!(".{p}")))
}

/// Un nombre de fichero que Windows acepte, sacado de la dirección o de la cabecera de la descarga.
pub fn nombre_seguro(n: &str) -> String {
    let n = percent_encoding::percent_decode_str(n).decode_utf8_lossy();
    let n = n.rsplit(['/', '\\']).next().unwrap_or("");
    let n: String = n.chars().map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c }).collect();
    let n = n.trim().trim_end_matches('.').to_string();
    if n.is_empty() { "parche.bin".into() } else { n }
}

fn nombre_de_cabecera(cd: &str) -> Option<String> {
    if let Some(i) = cd.find("filename*=UTF-8''") {
        return Some(cd[i + 17..].split(';').next()?.trim().to_string());
    }
    let i = cd.find("filename=")?;
    let n = cd[i + 9..].split(';').next()?.trim().trim_matches('"');
    (!n.is_empty()).then(|| n.to_string())
}

/// Baja `url` a la carpeta `dest`. Si ya estaba (mismo nombre y tamaño), no la vuelve a bajar.
pub fn descargar(url: &str, dest: &Path, log: &mut Vec<String>) -> Result<PathBuf, String> {
    let ag = agente()?;
    let mut u = Url::parse(url).map_err(|_| "La dirección de la descarga no es válida.".to_string())?;
    for _ in 0..8 {
        if !permitido(&u) {
            return Err(format!("Descarga no permitida desde {}.", u.host_str().unwrap_or("?")));
        }
        let host = u.host_str().unwrap_or("").to_string();
        let resp = match ag.get(u.as_str()).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => return Err(format!("El servidor respondió {code} ({host}).")),
            Err(_) => return Err(format!("No se pudo conectar con {host}.")),
        };
        let code = resp.status();
        if (300..400).contains(&code) {
            let loc = resp.header("Location").ok_or_else(|| format!("El servidor respondió {code} ({host})."))?;
            u = u.join(loc).map_err(|_| "Redirección no válida.".to_string())?;
            continue;
        }
        if code != 200 {
            return Err(format!("El servidor respondió {code} ({host})."));
        }
        let largo: Option<u64> = resp.header("Content-Length").and_then(|x| x.trim().parse().ok());
        if largo.is_some_and(|l| l > MAX_BYTES) {
            return Err("El fichero es demasiado grande.".into());
        }
        let nombre = resp
            .header("Content-Disposition")
            .and_then(nombre_de_cabecera)
            .or_else(|| u.path_segments().and_then(|mut s| s.next_back()).map(str::to_string))
            .map(|n| nombre_seguro(&n))
            .unwrap_or_else(|| "parche.bin".into());
        fs::create_dir_all(dest).map_err(|e| e.to_string())?;
        let fin = dest.join(&nombre);
        if let (Some(l), Ok(m)) = (largo, fs::metadata(&fin)) {
            if l > 0 && m.len() == l {
                log.push(format!("Ya estaba descargado: {nombre}"));
                return Ok(fin);
            }
        }
        let tmp = dest.join(format!("{nombre}.descargando"));
        let res = (|| -> Result<u64, String> {
            let mut r = resp.into_reader();
            let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 1 << 18];
            let mut total = 0u64;
            loop {
                let n = r.read(&mut buf).map_err(|_| format!("Se cortó la descarga desde {host}."))?;
                if n == 0 {
                    return Ok(total);
                }
                total += n as u64;
                if total > MAX_BYTES {
                    return Err("El fichero es demasiado grande.".into());
                }
                f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            }
        })();
        let total = match res {
            Ok(t) => t,
            Err(m) => {
                let _ = fs::remove_file(&tmp);
                return Err(m);
            }
        };
        let mut inicio = Vec::new();
        let _ = fs::File::open(&tmp).and_then(|f| f.take(512).read_to_end(&mut inicio));
        let cab = String::from_utf8_lossy(&inicio).trim_start().to_lowercase();
        if ["<!doctype", "<html", "<?xml", "{"].iter().any(|x| cab.starts_with(x)) {
            let _ = fs::remove_file(&tmp);
            return Err("Se descargó una página web en lugar del parche.".into());
        }
        let _ = fs::remove_file(&fin);
        mover(&tmp, &fin)?;
        log.push(format!("Descargado: {nombre} ({} KB)", total.div_ceil(1024)));
        return Ok(fin);
    }
    Err("Demasiadas redirecciones.".into())
}

/// GitHub: el parche está en los ficheros de la última versión publicada (o de la etiqueta indicada).
pub fn urls_github(repo: &str, tag: &str) -> Result<Vec<String>, String> {
    let api = if tag.is_empty() {
        format!("https://api.github.com/repos/{repo}/releases/latest")
    } else {
        format!("https://api.github.com/repos/{repo}/releases/tags/{tag}")
    };
    let sin_version = || format!("GitHub no tiene una versión publicada con ficheros para {repo}.");
    let rel: Value = agente()?
        .get(&api)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| sin_version())?
        .into_json()
        .map_err(|_| sin_version())?;
    const ORDEN: [&str; 10] = ["bps", "xdelta", "ups", "ips", "ppf", "zip", "7z", "rar", "aps", "vcdiff"];
    let mut assets: Vec<(usize, String)> = rel["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let n = a["name"].as_str()?.to_lowercase();
            let ext = n.rsplit('.').next()?;
            let pos = ORDEN.iter().position(|x| *x == ext)?;
            // Fuera el código fuente y las herramientas de los traductores.
            let tools = n.match_indices("tool").any(|(i, _)| !n[i + 4..].trim_start_matches('s').starts_with(|c: char| c.is_alphanumeric()));
            if n.contains("source") || n.contains("src") || n.contains("herramienta") || tools {
                return None;
            }
            Some((pos, a["browser_download_url"].as_str()?.to_string()))
        })
        .collect();
    if assets.is_empty() {
        return Err(format!("La versión publicada de {repo} no trae ningún parche."));
    }
    assets.sort_by_key(|(p, _)| *p);
    Ok(assets.into_iter().map(|(_, u)| u).collect())
}

/// Donde se publica el programa.
pub const REPOSITORIO: &str = "alucardog-dotcom/biblioteca-traducciones";

fn numeros(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(['.', '-']).map_while(|x| x.parse().ok()).collect()
}

/// La última versión publicada en GitHub, si es más nueva que `actual`: {version, url}.
pub fn version_nueva(actual: &str) -> Option<Value> {
    let rel: Value = agente()
        .ok()?
        .get(&format!("https://api.github.com/repos/{REPOSITORIO}/releases/latest"))
        .set("Accept", "application/vnd.github+json")
        .call()
        .ok()?
        .into_json()
        .ok()?;
    let tag = rel["tag_name"].as_str()?;
    (numeros(tag) > numeros(actual)).then(|| serde_json::json!({"version": tag.trim_start_matches(['v', 'V']), "url": rel["html_url"]}))
}

/// Los paquetes de RetroAchievements traen a veces varias traducciones o versiones: se sacan solo los
/// ficheros de esta ficha.
pub fn elegidos(siete_z: &Path, paquete: &Path, nombres: &[String], dest: &Path, temp: &Path) -> Result<Vec<PathBuf>, String> {
    let t = Temporal::nueva(temp)?;
    descomprimir(siete_z, paquete, &t.0, None)?;
    let mut out = vec![];
    for n in nombres {
        let src = n.split(['/', '\\']).fold(t.0.clone(), |a, x| a.join(x));
        if !src.is_file() {
            continue;
        }
        fs::create_dir_all(dest).map_err(|e| e.to_string())?;
        let dst = dest.join(nombre_seguro(&src.file_name().unwrap_or_default().to_string_lossy()));
        fs::copy(&src, &dst).map_err(|e| e.to_string())?;
        out.push(dst);
    }
    Ok(out)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn nombres() {
        assert_eq!(nombre_seguro("translations%2FArcade%2FFatal%20Fury%20%28E%29.zip"), "Fatal Fury (E).zip");
        assert_eq!(nombre_seguro("a:b?.ips"), "a_b_.ips");
        assert_eq!(nombre_seguro(".."), "parche.bin");
        assert_eq!(nombre_de_cabecera("attachment; filename=\"Juego (Es).zip\""), Some("Juego (Es).zip".into()));
        assert_eq!(nombre_de_cabecera("attachment; filename*=UTF-8''Pok%C3%A9mon.bps"), Some("Pok%C3%A9mon.bps".into()));
        assert_eq!(nombre_seguro("Pok%C3%A9mon.bps"), "Pokémon.bps");
    }

    #[test]
    fn versiones() {
        assert!(numeros("v0.5.0") > numeros("0.4.1"));
        assert!(numeros("0.10.0") > numeros("0.9.3"));
        assert!(numeros("0.4.1") == numeros("v0.4.1"));
        assert!(numeros("1.0.0-beta") > numeros("0.9"));
    }

    #[test]
    fn sitios() {
        for (u, ok) in [
            ("https://traduccioneswave.top/hacks/x.zip", true),
            ("https://ia800.us.archive.org/x.zip", true),
            ("https://objects.githubusercontent.com/x", true),
            ("http://github.com/x", false),
            ("https://evilgithub.com/x", false),
            ("https://mega.nz/x", false),
        ] {
            assert_eq!(permitido(&Url::parse(u).unwrap()), ok, "{u}");
        }
    }
}
