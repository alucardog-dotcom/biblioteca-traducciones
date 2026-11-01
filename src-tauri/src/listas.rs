//! Tus listas de juegos: Favoritos y las que crees tú, para volver a ellos más tarde. Se guardan en
//! <datos>\listas.json. Cada juego va con la clave que usa la ventana para juntar sus traducciones y con
//! la traducción que tenías elegida al guardarlo.

use crate::rutas::Rutas;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

const MAX_LISTAS: usize = 200;
const MAX_JUEGOS: usize = 10_000;

fn fichero(r: &Rutas) -> PathBuf {
    r.datos.join("listas.json")
}

/// Se queda solo con lo que se espera (textos con un tamaño razonable, sin juegos ni listas repetidos)
/// y con Favoritos siempre la primera.
fn limpiar(v: &Value) -> Value {
    let texto = |x: Option<&Value>, max: usize| x.and_then(Value::as_str).map(|s| s.trim().chars().take(max).collect::<String>()).filter(|s| !s.is_empty());
    let mut ids = HashSet::new();
    let mut listas: Vec<Value> = Vec::new();
    for l in v.get("listas").and_then(Value::as_array).into_iter().flatten().take(MAX_LISTAS) {
        let Some(id) = texto(l.get("id"), 40) else { continue };
        if !ids.insert(id.clone()) {
            continue;
        }
        let mut vistos = HashSet::new();
        let juegos: Vec<Value> = l.get("juegos").and_then(Value::as_array).into_iter().flatten()
            .filter_map(|j| {
                let k = texto(j.get("k"), 300)?;
                vistos.insert(k.clone()).then(|| json!({"k": k, "t": texto(j.get("t"), 400).unwrap_or_default(), "f": j.get("f").and_then(Value::as_u64).unwrap_or(0)}))
            })
            .take(MAX_JUEGOS)
            .collect();
        let nombre = if id == "fav" { "Favoritos".to_string() } else { texto(l.get("nombre"), 80).unwrap_or_else(|| "Sin nombre".into()) };
        listas.push(json!({"id": id, "nombre": nombre, "juegos": juegos}));
    }
    if !ids.contains("fav") {
        listas.push(json!({"id": "fav", "nombre": "Favoritos", "juegos": []}));
    }
    listas.sort_by_key(|l| l["id"] != "fav");
    json!({"listas": listas})
}

pub fn leer(r: &Rutas) -> Value {
    limpiar(&fs::read(fichero(r)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null))
}

/// Guarda todas las listas de una vez (la ventana manda siempre el conjunto entero).
pub fn guardar(r: &Rutas, v: &Value) -> Result<Value, String> {
    let limpio = limpiar(v);
    let nuevo = fichero(r).with_extension("json.nuevo");
    fs::write(&nuevo, serde_json::to_vec_pretty(&limpio).unwrap_or_default())
        .and_then(|_| fs::rename(&nuevo, fichero(r)))
        .map_err(|e| format!("No se pudo guardar la lista: {e}"))?;
    Ok(limpio)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn favoritos_siempre_primero() {
        let v = limpiar(&Value::Null);
        assert_eq!(v, json!({"listas": [{"id": "fav", "nombre": "Favoritos", "juegos": []}]}));
        let v = limpiar(&json!({"listas": [
            {"id": "l1", "nombre": "  Para el verano  ", "juegos": [{"k": "snes|chrono trigger", "t": "snes|Chrono Trigger|Rod Merida", "f": 5}, {"k": "snes|chrono trigger"}]},
            {"id": "fav", "nombre": "Otra cosa", "juegos": [{"k": "gba|3 mother"}, {"t": "sin clave"}]},
            {"id": "l1", "nombre": "Repetida", "juegos": []},
            {"nombre": "Sin id"},
        ]}));
        let l = v["listas"].as_array().unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0]["nombre"], "Favoritos");
        assert_eq!(l[0]["juegos"], json!([{"k": "gba|3 mother", "t": "", "f": 0}]));
        assert_eq!(l[1]["nombre"], "Para el verano");
        assert_eq!(l[1]["juegos"].as_array().unwrap().len(), 1);
    }
}
