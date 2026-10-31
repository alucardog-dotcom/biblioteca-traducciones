# Biblioteca de traducciones

Programa para Windows que busca tus juegos, te dice cuáles tienen traducción al español hecha por aficionados y la aplica con un clic. **Nunca descarga juegos**: solo los parches de traducción, que se aplican sobre tu propia copia, y tu copia queda intacta.

![Descubrir: el catálogo con sus portadas](docs/captura-descubrir.png)

## Qué hace

- **Catálogo** de 6.109 traducciones al español de 5.323 juegos: consolas y ordenadores clásicos (NES, Super Nintendo, Mega Drive, Game Boy, PlayStation, PS2, Nintendo DS…), MS-DOS y PC. Cada juego tiene una sola ficha, y dentro eliges entre sus traducciones.
- **Busca tus juegos** en las carpetas que elijas: ROMs sueltas o en .zip, .7z y .rar; discos .cue/.bin, .gdi y .chd. También encuentra los juegos de PC que tienes instalados con Steam, GOG y Epic.
- **Comprueba que tu copia es la buena**: por su CRC32 sabe si es la versión que pide la traducción, también sin la cabecera de NES o de SNES.
- **Descarga el parche y lo aplica**: IPS, BPS, UPS, PPF, APS y xdelta. Saca el juego de su comprimido o de su .chd y devuelve los discos traducidos en .chd. El traducido queda junto al original con la etiqueta `[T-Es]`, o en la carpeta que elijas.
- **Sin descarga directa**: abre la página de la traducción y, en cuanto bajas el parche, lo aplica él solo.
- **Juegos de PC**: sus traducciones se instalan a mano. La biblioteca te dice cuáles tienes y te lleva a la página de la traducción y a la carpeta del juego.
- **Portadas** de libretro-thumbnails, Steam y LaunchBox. Si falta alguna, puedes poner la tuya: elegir una imagen, pegarla con Ctrl+V o soltarla sobre el juego.
- **Con RetroBat**, el botón Jugar abre el juego traducido con su emulador.

![La ficha de un juego con varias traducciones](docs/captura-ficha.png)

## Instalar

Descarga el instalador (`…_x64-setup.exe`) de la [última versión](../../releases/latest) y ábrelo. Funciona en Windows 10 y 11 de 64 bits.

El instalador no está firmado, así que Windows puede avisar con «Windows protegió su PC». Para instalarlo, pulsa **Más información** y luego **Ejecutar de todas formas**.

Al abrirlo por primera vez te pregunta en qué carpetas tienes tus juegos. El catálogo se pone al día solo cada vez que lo abres, sin reinstalar nada. Cuando sale una versión nueva del programa, aparece un aviso abajo a la izquierda.

## De dónde salen las traducciones

Las traducciones son de sus autores. Este programa solo las encuentra y las aplica, y las descarga de su sitio original cuando ese sitio lo permite: [Traducciones Wave](https://traduccioneswave.top), la copia de romhacking.net de 2021 en [archive.org](https://archive.org/details/rhdn-20210914), GitHub y la [copia de parches de RetroAchievements](https://github.com/RetroAchievements/RAPatches). Para el resto, la ficha lleva a la página de la traducción.

El catálogo se ha hecho con los listados de romhacking.net, Traducciones Wave, Romhack Plaza, TraduSquare, Romhack Hispano, El Otro Lado, Steam Community, RetroAchievements, Abandonsocios, Parches en Castellano y blogs y repositorios de traductores. Si eres autor de una traducción y quieres corregir su ficha o que no aparezca, [abre una incidencia](../../issues).

No se enlazan los sitios que reparten juegos ya parcheados o completos. Las traducciones hechas con IA o con traducción automática van marcadas, y hay un filtro para ocultarlas.

## Privacidad

El programa no envía datos tuyos a ningún sitio. Solo se conecta para:

- **Portadas:** thumbnails.libretro.com, la CDN de Steam e images.launchbox-app.com.
- **Parches:** traduccioneswave.top, archive.org y github.com.
- **Versión y catálogo nuevos:** api.github.com y raw.githubusercontent.com, para ver si hay una versión nueva del programa y para traer el catálogo más reciente.

Tus ajustes y tus datos se guardan en tu equipo: `%APPDATA%\es.traducciones.biblioteca` y `%LOCALAPPDATA%\es.traducciones.biblioteca`.

## Compilar

Cómo está hecho y cómo compilarlo: [DESARROLLO.md](DESARROLLO.md). Está hecho con [Tauri 2](https://tauri.app) (Rust y una ventana en HTML, CSS y JavaScript) y se inspira en [RecompLauncher](https://github.com/elripalda/RecompLauncher), de Ripalda, aunque no usa su código.

## Licencia

El código de este programa es software libre: puedes usarlo, estudiarlo, cambiarlo y repartirlo según la [licencia GNU GPL versión 3](LICENSE) o, si lo prefieres, cualquier versión posterior.

Lleva dentro 7-Zip, xdelta3 y chdman (de MAME), cada uno con su licencia: ver [LICENCIAS.txt](src-tauri/recursos/herramientas/LICENCIAS.txt). Las portadas son de sus propietarios. Este programa no está relacionado con ninguna de las empresas de los juegos.
