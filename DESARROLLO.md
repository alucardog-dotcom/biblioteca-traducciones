# Desarrollo

Cómo está hecha la Biblioteca de traducciones y cómo compilarla. Lo que hace el programa está en el [README](README.md).

Está hecha con [Tauri 2](https://tauri.app): el motor en Rust (`src-tauri/`) y la ventana en HTML, CSS y JavaScript sin frameworks (`ui/`). De RecompLauncher solo toma la idea general; el código es propio.

## Cómo funciona

| Qué | Dónde |
|---|---|
| Catálogo | Va dentro del programa, en `src-tauri/recursos/catalogo`: `traducciones_fan.csv` (las traducciones) y `centro_datos.json` (portadas, descargas directas y AppID de Steam de cada ficha) |
| Catálogo al día | `catalogo.rs`. Al abrir, y luego cada seis horas (`catalogo_al_dia` en `lib.rs`), compara el sha de git de esos dos ficheros con los de la rama `main` de este repositorio (API de GitHub). Si son otros, baja los nuevos a `<datos>\catalogo`, comprueba su sha y que se leen bien, y la ventana los recarga. Una versión nueva del programa vuelve a usar el suyo. El botón «Buscar actualizaciones» de Ajustes (comando `buscar_actualizaciones`) hace esa misma comprobación a mano junto con la de versión nueva del programa. Las portadas a mano que aún no lleva dentro el programa se traen de `ui/portadas` del repositorio |
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

Al publicar una etiqueta `v*` en GitHub, GitHub Actions compila el instalador, lo firma (ver abajo) y lo sube a la versión (`.github/workflows/publicar.yml`).

## Firma del instalador

El instalador saldrá firmado con el certificado de [SignPath Foundation](https://signpath.org), que firma gratis programas de código libre compilados en GitHub Actions, cuando acepten el proyecto. **Estado:** solicitado en noviembre de 2026 y denegado por falta de visibilidad (miran estrellas, forks, colaboradores, artículos y hilos en Reddit o YouTube); invitan a volver a pedirlo en <https://signpath.org/apply> cuando el programa tenga difusión. Su plan de pago (500 $ al año) solo da certificados a empresas registradas. Al pedirlo otra vez, el README tiene que llevar en «Code signing policy» las frases que exigen, tal como estaban en el commit 42be214: la atribución «Free code signing provided by SignPath.io, certificate by SignPath Foundation», los roles del equipo (committers and reviewers, approvers) y la frase de privacidad «This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it».

SignPath no firma dentro de instaladores NSIS, así que se firma el instalador entero y el `.exe` del programa que va dentro queda sin firmar (ver Pendiente). Las condiciones están en <https://signpath.org/terms>: licencia libre, doble factor en GitHub y en SignPath, la sección «Code signing policy» del README y aprobar cada firma a mano.

El flujo `publicar.yml` lo hace así: guarda el instalador como artefacto del flujo (`actions/upload-artifact`), lo manda a firmar (`signpath/github-action-submit-signing-request`), espera a que alguien del proyecto apruebe la petición en signpath.io (**media hora como mucho**; si se pasa, el trabajo falla y se vuelve a lanzar con `gh run rerun --failed <id>`) y sube el instalador firmado a la versión. Si en el repositorio no está la variable `SIGNPATH_ORGANIZATION_ID`, se salta la firma y sube el instalador sin firmar, como antes.

Lo que hay que tener configurado:

- En el repositorio (Settings › Secrets and variables › Actions): el secreto `SIGNPATH_API_TOKEN` (token de API de un usuario de SignPath con permiso de *submitter*) y las variables `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG` y `SIGNPATH_SIGNING_POLICY_SLUG` (la política de firma de versiones, normalmente `release-signing`).
- En SignPath: el sistema de compilación de confianza *GitHub.com* añadido a la organización y enlazado al proyecto; el proyecto enlazado a este repositorio; y esta configuración de artefacto, que exige que el fichero firmado lleve el nombre del programa y la versión que le pasa el flujo:

```xml
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <parameters>
    <parameter name="version" required="true" />
  </parameters>
  <pe-file product-name="Biblioteca de traducciones" product-version="${version}">
    <authenticode-sign />
  </pe-file>
</artifact-configuration>
```

En las notas de cada versión conviene poner el enlace a la política: `Instalador firmado por SignPath Foundation: [Code signing policy](https://github.com/alucardog-dotcom/biblioteca-traducciones#code-signing-policy)`. Y cuando salga la primera versión firmada, quitar del README la nota «estamos en ello» y el aviso de que el instalador no está firmado.

## Pruebas

`cargo test` en `src-tauri`. Las pruebas crean juegos y parches inventados y comprueban cada formato, las cabeceras de NES y SNES, los comprimidos y los discos .cue/.bin y .chd con xdelta. Usan las herramientas de `src-tauri/recursos/herramientas`.

Para inspeccionar la ventana abierta, arranca el programa con `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` y conéctate a ese puerto con las herramientas de desarrollo de Chrome o Edge.

## El catálogo

El catálogo se mantiene aparte, con unos scripts de Python que todavía no están en este repositorio. Leen los listados de cada fuente y generan `traducciones_fan.csv` y `centro_datos.json`. `tools/preparar_recursos.py` copia el resultado dentro del programa:

- el catálogo, a `src-tauri/recursos/catalogo`;
- las portadas elegidas a mano, a `ui/portadas`;
- 7-Zip, chdman y xdelta3, a `src-tauri/recursos/herramientas`.

Las portadas que alguien pone a mano en el programa pueden pasar al catálogo para que las vea todo el mundo. Se guardan por clave de juego, la misma con la que la ventana junta las traducciones (`claveJuego` en `ui/app.js`).

Para poner al día el catálogo de todos basta con subir a `main` los ficheros nuevos de `src-tauri/recursos/catalogo` (y las portadas de `ui/portadas`): los programas ya instalados los descargan al abrirse, cada seis horas o al pulsar «Buscar actualizaciones». No hace falta publicar una versión.

## La web

`tools/web.py` genera la web a partir del catálogo: la portada, el buscador (`web/catalogo.js`, que lee `catalogo/juegos.json`), una página por juego y otra por sistema, más `sitemap.xml` y `robots.txt`. Junta las traducciones en juegos y elige las portadas igual que la ventana. Solo usa la biblioteca estándar de Python:

```
python tools/web.py          (la deja en _web; para verla: python -m http.server -d _web)
```

Las páginas de los juegos de PC que están en Steam (los que tienen AppID) llevan un enlace de afiliado a la búsqueda de Instant Gaming (`INSTANT_GAMING` en `tools/web.py`), con su aviso. La ficha del programa lo enseña también cuando no tienes el juego (`INSTANT_GAMING` en `ui/app.js`).

Los juegos de PC que no están en Steam pero tienen página oficial conocida (itch.io, Game Jolt o la web de su autor; campo `pj` del catálogo, con `pj18` si es para adultos) llevan en su lugar un enlace a esa página, sin afiliado, también en el programa cuando no tienes el juego.

Las traducciones de PC y de las consolas actuales (Switch, PS3, PS4, Xbox 360…; `MODERNAS` en `ui/app.js` y en `tools/web.py`) se instalan a mano: ni la web ni el programa las marcan como «Un clic» aunque su descarga sea directa.

El flujo `web.yml` la genera y la publica en GitHub Pages cada vez que cambia en `main` el catálogo, las portadas a mano, las capturas o la propia web.

Los botones «Descargar» de la web bajan el instalador directamente: enlazan a `releases/latest/download/Biblioteca-de-traducciones-setup.exe`, y `publicar.yml` sube el instalador solo con ese nombre fijo (hasta la 0.7.8 lo subía también con el nombre con versión, y en la página de la versión salían dos ficheros iguales). La versión se ve en la página de la release y en Ajustes › Acerca de; el SHA-256 lo enseña GitHub junto al fichero.

Las estadísticas de la web van con [GoatCounter](https://www.goatcounter.com/) (gratuito para webs sin ánimo de lucro, sin cookies, por lo que no hace falta aviso de cookies): en `tools/web.py`, la constante `GOATCOUNTER` lleva el código del sitio (lo que va antes de `.goatcounter.com`); vacía, la web no carga ningún script. Cuenta las páginas vistas, de dónde viene la gente y los clics en los botones de descarga (`data-goatcounter-click`). Las descargas totales de cada versión las cuenta GitHub (`gh api repos/alucardog-dotcom/biblioteca-traducciones/releases --jq '.[] | {tag_name, assets: [.assets[] | {name, download_count}]}'`), y las búsquedas con las que aparece la web, Google Search Console. El dominio es `bibliotecadetraducciones.com` (comprado en Nominalia el 07-11-2026, con `bibliotecadetraducciones.es`; renuevan el 07-11-2027). Está en `web/CNAME` (de ahí lo leen `web.py` y `preparar_centro.py`, que pone esa dirección en las miniaturas del catálogo) y en Settings › Pages del repositorio. En Nominalia, el `.com` lleva los registros de GitHub Pages: cuatro A en la raíz (185.199.108.153, 185.199.109.153, 185.199.110.153 y 185.199.111.153), sus cuatro AAAA (2606:50c0:8000::153 a 2606:50c0:8003::153) y `www` como CNAME a `alucardog-dotcom.github.io`; el `.es` solo redirige al `.com`. La dirección antigua de GitHub Pages redirige sola al dominio.

## Pendiente

- Publicar los scripts del catálogo.
- Modo para mando y tele, como el Big Picture de RecompLauncher.
- Guardar en disco las portadas descargadas, para verlas sin conexión.
- Volver a pedir la firma a SignPath Foundation cuando haya difusión (anuncios publicados, estrellas, hilos, descargas).
- Firmar también el `.exe` del programa que va dentro del instalador: compilar sin empaquetar (`tauri build --no-bundle`), firmarlo en SignPath, empaquetarlo con `tauri bundle` y firmar el instalador; dos aprobaciones por versión.
