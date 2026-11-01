# Desarrollo

Cómo está hecha la Biblioteca de traducciones y cómo compilarla. Lo que hace el programa está en el [README](README.md).

Está hecha con [Tauri 2](https://tauri.app): el motor en Rust (`src-tauri/`) y la ventana en HTML, CSS y JavaScript sin frameworks (`ui/`). De RecompLauncher solo toma la idea general; el código es propio.

## Cómo funciona

| Qué | Dónde |
|---|---|
| Catálogo | Va dentro del programa, en `src-tauri/recursos/catalogo`: `traducciones_fan.csv` (las traducciones) y `centro_datos.json` (portadas, descargas directas y AppID de Steam de cada ficha) |
| Catálogo al día | `catalogo.rs`. Al abrir, compara el sha de git de esos dos ficheros con los de la rama `main` de este repositorio (API de GitHub). Si son otros, baja los nuevos a `<datos>\catalogo`, comprueba su sha y que se leen bien, y la ventana los recarga. Una versión nueva del programa vuelve a usar el suyo. Las portadas a mano que aún no lleva dentro el programa se traen de `ui/portadas` del repositorio |
| Ventana | `ui/app.js`. Junta las fichas en juegos (mismo sistema y título, o mismo AppID de Steam), empareja tus juegos con las traducciones (por CRC32, por AppID o por el nombre) y pinta las secciones |
| Buscar tus juegos | `escaneo.rs`. Saca el CRC32 de cada ROM, lee el índice de los .zip sin descomprimirlos y usa 7-Zip para los .7z y .rar. Guarda una caché para que las siguientes búsquedas vayan rápidas. Los juegos de PC salen del registro y los manifiestos de Steam, GOG y Epic |
| Bajar el parche | `descarga.rs`. Solo de Traducciones Wave, archive.org y GitHub (también la API de versiones de GitHub), comprobando cada redirección. De los paquetes de RetroAchievements saca solo los ficheros de la ficha |
| Aplicar el parche | `parcheo.rs` y `motor.rs`. Comprueba por CRC32 que tu juego es la versión que pide la traducción, también sin la cabecera de NES o de SNES. Saca el juego de su .zip/.7z/.rar o de su .chd y monta el disco traducido en un .chd. IPS, BPS, UPS, PPF y APS los aplica el propio programa; xdelta, el `xdelta3.exe` que lleva dentro, igual que 7-Zip y chdman |
| Trabajos | `trabajos.rs`. Una cola: buscar juegos y traducir, de uno en uno. Vigila la carpeta Descargas cuando una traducción no tiene descarga directa |
| Portadas propias | `portadas.rs`. Las imágenes se copian a `portadas\` en tus datos y se apuntan en `portadas_propias.json`; la ventana las ve con el protocolo `asset` de Tauri |
| Favoritos y listas | `listas.rs`. Se guardan en `listas.json` en tus datos: cada juego con su clave de juego (la misma con la que la ventana junta sus traducciones) y la traducción que tenías elegida. La ventana manda siempre todas las listas juntas y aquí se limpian (Favoritos siempre existe y va la primera) |
| Tus datos | `%LOCALAPPDATA%\es.traducciones.biblioteca`: trabajos y su registro, parches descargados, la lista de tus juegos, tus portadas y tus listas |
| Ajustes | `%APPDATA%\es.traducciones.biblioteca\ajustes.json`. Si no existe, la ventana da la bienvenida y pregunta dónde están los juegos |

Las portadas se prueban en orden hasta que una carga: la tuya, la elegida a mano para el catálogo (va dentro del programa, en `ui/portadas`), la de libretro-thumbnails (por el nombre No-Intro o Redump) o la de Steam, la de la base de datos pública de LaunchBox y la pantalla de título.

## Compilar

Hace falta Node.js, Rust (`rustup`, versión estable) y las herramientas de C++ de Visual Studio (Build Tools 2019 o posteriores).

```bash
npm install
npm run build:prueba                  # prueba rápida: src-tauri\target\debug\biblioteca-traducciones.exe
npm run build                         # instalador: src-tauri\target\release\bundle\nsis\
```

Compila siempre con `npm run build`, que llama a `tools\compilar.ps1`, y no con `npx tauri build` directamente. Ese script le dice al compilador que cambie por nombres genéricos las rutas de tu máquina (tu carpeta de usuario, la de Rust y la del proyecto). Si no, el `.exe` las lleva dentro.

Al publicar una etiqueta `v*` en GitHub, GitHub Actions compila el instalador y lo sube a la versión (`.github/workflows/publicar.yml`).

## Pruebas

`cargo test` en `src-tauri`. Las pruebas crean juegos y parches inventados y comprueban cada formato, las cabeceras de NES y SNES, los comprimidos y los discos .cue/.bin y .chd con xdelta. Usan las herramientas de `src-tauri/recursos/herramientas`.

Para inspeccionar la ventana abierta, arranca el programa con `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` y conéctate a ese puerto con las herramientas de desarrollo de Chrome o Edge.

## El catálogo

El catálogo se mantiene aparte, con unos scripts de Python que todavía no están en este repositorio. Leen los listados de cada fuente y generan `traducciones_fan.csv` y `centro_datos.json`. `tools/preparar_recursos.py` copia el resultado dentro del programa:

- el catálogo, a `src-tauri/recursos/catalogo`;
- las portadas elegidas a mano, a `ui/portadas`;
- 7-Zip, chdman y xdelta3, a `src-tauri/recursos/herramientas`.

Las portadas que alguien pone a mano en el programa pueden pasar al catálogo para que las vea todo el mundo. Se guardan por clave de juego, la misma con la que la ventana junta las traducciones (`claveJuego` en `ui/app.js`).

Para poner al día el catálogo de todos basta con subir a `main` los ficheros nuevos de `src-tauri/recursos/catalogo` (y las portadas de `ui/portadas`): los programas ya instalados los descargan al abrirse. No hace falta publicar una versión.

## La web

`tools/web.py` genera la web a partir del catálogo: la portada, el buscador (`web/catalogo.js`, que lee `catalogo/juegos.json`), una página por juego y otra por sistema, más `sitemap.xml` y `robots.txt`. Junta las traducciones en juegos y elige las portadas igual que la ventana. Solo usa la biblioteca estándar de Python:

```
python tools/web.py          (la deja en _web; para verla: python -m http.server -d _web)
```

El flujo `web.yml` la genera y la publica en GitHub Pages cada vez que cambia en `main` el catálogo, las portadas a mano, las capturas o la propia web. Para usar un dominio propio, pon el dominio en `web/CNAME` y en Settings › Pages del repositorio, y apunta el DNS del dominio a GitHub Pages.

## Pendiente

- Publicar los scripts del catálogo.
- Modo para mando y tele, como el Big Picture de RecompLauncher.
- Guardar en disco las portadas descargadas, para verlas sin conexión.
- Firmar el instalador.
