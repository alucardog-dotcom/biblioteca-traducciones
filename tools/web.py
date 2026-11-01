# Genera la web de la Biblioteca de traducciones (GitHub Pages) a partir del catálogo: la portada, el
# buscador, una página por juego y otra por sistema. Solo usa la biblioteca estándar de Python.
# Uso: python tools/web.py [carpeta de salida, _web por defecto]
# El dominio va en web/CNAME; si no está, la web queda en la dirección de GitHub Pages.
import csv, html, json, os, re, shutil, sys, unicodedata
from datetime import date
from urllib.parse import quote

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CAT = os.path.join(RAIZ, 'src-tauri', 'recursos', 'catalogo')
WEB = os.path.join(RAIZ, 'web')
REPO = 'https://github.com/alucardog-dotcom/biblioteca-traducciones'
DESCARGA = REPO + '/releases/latest'
INCIDENCIAS = REPO + '/issues'
DONAR = 'https://paypal.me/btraducciones'
PC = {'dos', 'windows', 'scummvm', 'easyrpg', 'pc'}
e = html.escape

# ---------------------------------------------------------------- nombres (como tokens() en ui/app.js)
STOP = set('the a an el la los las de del of and y en to traduccion traducido traducida translation translated spanish '
           'espanol castellano es esp spa parche patch fix final rev beta completa completo ips bps ups ppf xdelta aps rom '
           'iso track disc usa japan europe jap jp us eu'.split())
ROMAN = {'ii': '2', 'iii': '3', 'iv': '4', 'vi': '6', 'vii': '7', 'viii': '8', 'ix': '9'}


def sin_acentos(t):
    return ''.join(c for c in unicodedata.normalize('NFD', t) if not unicodedata.combining(c))


def tokens(t):
    if not t:
        return []
    s = re.sub(r'\([^)]*\)', ' ', re.sub(r'\[[^\]]*\]', ' ', t))
    s = re.sub(r'[^a-z0-9]+', ' ', re.sub(r"['’]", '', sin_acentos(s).lower()))
    out = []
    for w in s.split():
        if re.fullmatch(r'v\d+', w) or (re.fullmatch(r'\d+[a-z]?', w) and len(w) > 4) or w in STOP:
            continue
        w = ROMAN.get(w, w)
        if w not in out:
            out.append(w)
    return out


def slug(t):
    s = re.sub(r'[^a-z0-9]+', '-', re.sub(r"['’]", '', sin_acentos(t).lower())).strip('-')
    return s[:70].strip('-') or 'juego'


def sistemas_app():
    """Los nombres de los sistemas, los mismos que enseña el programa (SYS en ui/app.js)."""
    t = open(os.path.join(RAIZ, 'ui', 'app.js'), encoding='utf-8').read()
    cuerpo = re.search(r'const SYS = \{(.*?)\};', t).group(1)
    return {k.strip("'"): v for k, v in re.findall(r"('?\w+'?):'([^']*)'", cuerpo)}


# ---------------------------------------------------------------- catálogo
def cargar():
    filas = list(csv.DictReader(open(os.path.join(CAT, 'traducciones_fan.csv'), encoding='utf-8-sig')))
    centro = json.load(open(os.path.join(CAT, 'centro_datos.json'), encoding='utf-8'))
    for f in filas:
        f['x'] = centro['filas'].get(f"{f['sistema']}|{f['juego']}|{f['grupo']}", {})
    return filas, centro['sistemas']


def con_portada(f):
    return bool(f['x'].get('pm') or f['x'].get('p') or f['x'].get('steam') or f['x'].get('pu'))


def clave_juego(f):
    if f['x'].get('steam'):
        return f"steam|{f['x']['steam']}"
    t = tokens(f['juego'])
    return f['sistema'] + '|' + (' '.join(sorted(t)) if t else f['juego'].lower())


def agrupar(filas):
    """Un juego, una página: las traducciones del mismo juego van juntas, como en el programa."""
    grupos = {}
    for f in filas:
        grupos.setdefault(clave_juego(f), []).append(f)
    out = []
    for fs in grupos.values():
        rep = sorted(fs, key=lambda f: (not con_portada(f), '(' in f['juego']))[0]
        cover = next((f for f in fs if f['x'].get('pm')), None) or next((f for f in fs if con_portada(f)), rep)
        out.append({'titulo': rep['juego'], 's': rep['sistema'], 'filas': sorted(fs, key=orden), 'cover': cover})
    return out


def es_ia(f):
    return f['notas'].startswith('[IA]')


def directa(f):
    return bool(f['x'].get('d'))


def orden(f):
    """Primero la de un clic, la completa, la que no es de IA y la más reciente."""
    puntos = (100 if directa(f) else 0) + (50 if re.search('complet', f['estado'], re.I) else 0) + (0 if es_ia(f) else 10)
    return (-puntos, '~' if not f['fecha'] else ''.join(chr(255 - ord(c)) for c in f['fecha']))


def encode(s):
    return quote(s, safe="-_.!~*'()")


def portadas(gr, libretro, raiz):
    """Las direcciones de la portada, en el orden en que las prueba el programa."""
    x, s = gr['cover']['x'], gr['cover']['sistema']
    sis = x.get('ps') or libretro.get(s)
    tipo = x.get('pt') or 'Named_Boxarts'
    alt = 'Named_Titles' if tipo == 'Named_Boxarts' else 'Named_Snaps'

    def thumb(t):
        nombre = re.sub(r'[&*/:`<>?\\|"]', '_', x['p'])
        return f'https://thumbnails.libretro.com/{encode(sis)}/{t}/{encode(nombre)}.png'

    # La puesta a mano ya viene como «portadas/<fichero>», igual que en ui/.
    urls = [raiz + x['pm']] if x.get('pm') else []
    if x.get('steam'):
        st = f"https://cdn.cloudflare.steamstatic.com/steam/apps/{encode(str(x['steam']))}/"
        urls += [st + 'library_600x900.jpg', x.get('pu'), st + 'header.jpg']
    elif x.get('p') and sis:
        urls += [thumb(tipo), x.get('pu'), thumb(alt)]
    else:
        urls.append(x.get('pu'))
    return [u for u in urls if u]


# ---------------------------------------------------------------- páginas
def pagina(raiz, titulo, descripcion, ruta, cuerpo, base, imagen='img/vista-previa.png', tipo='website'):
    canon = base + ruta
    return f'''<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{e(titulo)}</title>
<meta name="description" content="{e(descripcion)}">
<link rel="canonical" href="{e(canon)}">
<link rel="icon" href="{raiz}icono.svg" type="image/svg+xml">
<link rel="stylesheet" href="{raiz}estilo.css">
<meta property="og:type" content="{tipo}">
<meta property="og:title" content="{e(titulo)}">
<meta property="og:description" content="{e(descripcion)}">
<meta property="og:url" content="{e(canon)}">
<meta property="og:image" content="{e(base + imagen)}">
<meta property="og:locale" content="es_ES">
<meta name="twitter:card" content="summary_large_image">
<script>function sig(i){{var r=(i.dataset.alt||'').split(' ').filter(Boolean);if(!r.length){{i.parentNode.classList.add('sin');i.remove();return}}i.src=r.shift();i.dataset.alt=r.join(' ')}}</script>
</head>
<body>
<div class="franja" aria-hidden="true"></div>
<header class="barra">
  <a class="marca" href="{raiz or './'}"><img src="{raiz}icono.svg" alt="" width="32" height="32"><span>Biblioteca de traducciones</span></a>
  <nav aria-label="Principal">
    <a href="{raiz}catalogo/">Catálogo</a>
    <a href="{REPO}">GitHub</a>
    <a class="btn primario" href="{DESCARGA}">Descargar</a>
  </nav>
</header>
{cuerpo}
<footer class="pie">
  <div class="pie-in">
    <p><b>Biblioteca de traducciones</b> es gratuita y de código libre (<a href="{REPO}/blob/main/LICENSE">GPL-3.0</a>). Las traducciones son de sus autores: este programa solo las encuentra y las aplica, y nunca descarga juegos.</p>
    <p><a href="{REPO}">Código en GitHub</a> · <a href="{INCIDENCIAS}">Avisar de un error o de una traducción que falta</a> · <a href="{DONAR}">Invítame a un café</a></p>
    <p class="suave">No está relacionado con ninguna de las empresas de los juegos. Las portadas son de sus propietarios.</p>
  </div>
</footer>
</body>
</html>
'''


def chips_trad(f):
    out = []
    est = f['estado'].strip()
    if est:
        clase = 'ok' if re.search('complet|publicad', est, re.I) else 'aviso'
        out.append(f'<span class="chip {clase}">{e(est.capitalize())}</span>')
    if directa(f):
        out.append('<span class="chip clic" title="El programa la descarga y la aplica él solo">Un clic</span>')
    if es_ia(f):
        out.append('<span class="chip ia" title="Hecha con IA o con traducción automática">IA</span>')
    return ''.join(out)


def formato(f):
    fmt = f['formato_parche'].strip()
    return fmt.upper() if fmt.lower() in ('ips', 'bps', 'ups', 'ppf', 'aps', 'xdelta') else fmt.capitalize()


def tarjeta_trad(f):
    datos = []
    if f['version']:
        datos.append(('Versión', e(f['version'])))
    if f['fecha']:
        datos.append(('Fecha', e(f['fecha'])))
    if f['formato_parche']:
        datos.append(('Formato', e(formato(f))))
    roms = [r.strip() for r in f['rom_requerida'].split('|') if r.strip()]
    if roms:
        datos.append(('Pide la versión', '<br>'.join(e(r) for r in roms)))
    crcs = [c.strip().upper() for c in f['crc32'].split('|') if re.fullmatch(r'[0-9a-fA-F]{8}', c.strip())]
    if crcs:
        datos.append(('CRC32', ' · '.join(f'<code>{c}</code>' for c in crcs)))
    dl = ''.join(f'<dt>{k}</dt><dd>{v}</dd>' for k, v in datos)
    nota = re.sub(r'^\[IA\]\s*', '', f['notas']).strip()
    enlace = (f'<a class="btn" href="{e(f["url"])}" target="_blank" rel="noopener">Página de la traducción</a>'
              if f['url'] else '<p class="suave">No se conoce la página de esta traducción.</p>')
    return f'''<article class="trad">
  <div class="trad-h"><h3>{e(f['grupo'] or 'Autor desconocido')}</h3><div class="chips">{chips_trad(f)}</div></div>
  {f'<dl>{dl}</dl>' if dl else ''}
  {f'<p class="nota">{e(nota)}</p>' if nota else ''}
  {enlace}
</article>'''


def pagina_juego(gr, sysn, libretro, base):
    raiz = '../../../'
    titulo, s = gr['titulo'], gr['s']
    nombre_sis = sysn.get(s, s)
    grupos = list(dict.fromkeys(f['grupo'] for f in gr['filas'] if f['grupo']))
    n = len(gr['filas'])
    urls = portadas(gr, libretro, raiz)
    img = (f'<img src="{e(urls[0])}" data-alt="{e(" ".join(urls[1:]))}" alt="Portada de {e(titulo)}" '
           f'referrerpolicy="no-referrer" onerror="sig(this)">') if urls else ''
    quien = ', '.join(grupos[:3]) + (' y otros' if len(grupos) > 3 else '')
    desc = (f'{"Traducción" if n == 1 else f"{n} traducciones"} al español de {titulo} para {nombre_sis}'
            + (f', de {quien}' if quien else '') + '. Aplícala sobre tu copia con un clic con la Biblioteca de traducciones.')
    if s in PC:
        como = '''<ol class="pasos">
      <li>Descarga la Biblioteca de traducciones e instálala.</li>
      <li>Si tienes el juego en Steam, GOG o Epic, la biblioteca lo encuentra sola.</li>
      <li>Las traducciones de PC se instalan a mano: abre la ficha del juego y te lleva a la página de la traducción y a la carpeta del juego.</li>
    </ol>'''
    else:
        como = f'''<ol class="pasos">
      <li>Descarga la Biblioteca de traducciones e instálala.</li>
      <li>Dile en qué carpeta tienes tus juegos.</li>
      <li>Busca «{e(titulo)}» y pulsa <b>Traducir</b>. Comprueba que tu copia es la versión que pide la traducción y la aplica; tu original no se toca.</li>
    </ol>
    <p class="suave">Necesitas tu propia copia del juego: el programa no descarga juegos, solo los parches de traducción.</p>'''
    cuerpo = f'''<main class="juego">
  <nav class="migas" aria-label="Estás en"><a href="{raiz}">Inicio</a> › <a href="{raiz}catalogo/">Catálogo</a> › <a href="{raiz}sistema/{s}/">{e(nombre_sis)}</a></nav>
  <div class="juego-h">
    <div class="portada{'' if img else ' sin'}">{img}<span class="ph">{e(titulo)}</span></div>
    <div class="juego-t">
      <p class="eti">{e(nombre_sis)}</p>
      <h1>{e(titulo)} <span>en español</span></h1>
      <p class="lede">{'Hay una traducción' if n == 1 else f'Hay {n} traducciones'} al español hechas por aficionados{f": {e(quien)}" if quien else ''}.</p>
      <a class="btn primario grande" href="{DESCARGA}">Descargar el programa para Windows</a>
    </div>
  </div>
  <section class="bloque">
    <h2>{'La traducción' if n == 1 else 'Las traducciones'}</h2>
    <div class="trads">{''.join(tarjeta_trad(f) for f in gr['filas'])}</div>
  </section>
  <section class="bloque como">
    <h2>Cómo jugarlo en español</h2>
    {como}
  </section>
  <p class="suave">¿Algún dato está mal o eres el autor y quieres cambiar algo? <a href="{INCIDENCIAS}">Avísame en GitHub</a>.</p>
</main>'''
    return pagina(raiz, f'{titulo} en español ({nombre_sis}) · Biblioteca de traducciones', desc, gr['ruta'], cuerpo, base, tipo='article')


def pagina_sistema(s, juegos, sysn, base):
    raiz = '../../'
    nombre = sysn.get(s, s)
    n = sum(len(g['filas']) for g in juegos)
    letras = {}
    for g in sorted(juegos, key=lambda g: sin_acentos(g['titulo']).lower()):
        l = sin_acentos(g['titulo'])[:1].upper()
        letras.setdefault(l if l.isalpha() else '#', []).append(g)

    def item(g):
        n = len(g['filas'])
        return f'<li><a href="{raiz}{g["ruta"]}">{e(g["titulo"])}</a>' + (f' <span class="n">{n}</span>' if n > 1 else '') + '</li>'

    bloques = ''.join(f'<section class="letra"><h2>{e(l)}</h2><ul class="lista">{"".join(item(g) for g in gs)}</ul></section>'
                      for l, gs in letras.items())
    cuerpo = f'''<main class="sistema">
  <nav class="migas" aria-label="Estás en"><a href="{raiz}">Inicio</a> › <a href="{raiz}catalogo/">Catálogo</a></nav>
  <h1>Traducciones al español de {e(nombre)}</h1>
  <p class="lede">{n:,} {"traducción" if n == 1 else "traducciones"} de {len(juegos):,} {"juego" if len(juegos) == 1 else "juegos"}, hechas por aficionados. Elige un juego para ver sus traducciones.</p>
  {bloques}
</main>'''
    desc = f'{n:,} traducciones al español de juegos de {nombre} hechas por aficionados, listas para aplicar con un clic con la Biblioteca de traducciones.'
    return pagina(raiz, f'Juegos de {nombre} en español · Biblioteca de traducciones', desc, f'sistema/{s}/', cuerpo, base)


def chips_sistemas(por_sis, sysn, raiz):
    return ''.join(f'<a class="sis" href="{raiz}sistema/{s}/">{e(sysn.get(s, s))} <span>{len(sum((g["filas"] for g in gs), []))}</span></a>'
                   for s, gs in sorted(por_sis.items(), key=lambda kv: -sum(len(g['filas']) for g in kv[1])))


def pagina_catalogo(por_sis, sysn, base, n_trad, n_juegos):
    raiz = '../'
    opciones = ''.join(f'<option value="{s}">{e(sysn.get(s, s))}</option>' for s in sorted(por_sis, key=lambda s: sysn.get(s, s).lower()))
    cuerpo = f'''<main class="catalogo">
  <h1>Catálogo de traducciones</h1>
  <p class="lede">{n_trad:,} traducciones al español de {n_juegos:,} juegos. Busca el tuyo por su nombre, en inglés, en japonés romanizado o en español, o por el grupo que lo tradujo.</p>
  <form class="buscador" role="search" onsubmit="return false">
    <label class="sr" for="q">Buscar un juego</label>
    <input id="q" type="search" placeholder="Busca un juego o un grupo…" autocomplete="off" autofocus>
    <label class="sr" for="s">Sistema</label>
    <select id="s"><option value="">Todos los sistemas</option>{opciones}</select>
  </form>
  <p class="suave" id="cuenta" aria-live="polite"></p>
  <ul class="resultados" id="res"></ul>
  <button class="btn" id="mas" hidden>Ver más</button>
  <section class="bloque">
    <h2>Por sistema</h2>
    <div class="sistemas">{chips_sistemas(por_sis, sysn, raiz)}</div>
  </section>
</main>
<script>var SYS = {json.dumps(sysn, ensure_ascii=False)};</script>
<script src="catalogo.js" defer></script>'''
    desc = f'Busca entre {n_trad:,} traducciones al español de juegos hechas por aficionados, de la NES a los juegos de PC, y aplícalas con un clic.'
    return pagina(raiz, 'Catálogo de traducciones al español · Biblioteca de traducciones', desc, 'catalogo/', cuerpo, base)


def pagina_inicio(por_sis, sysn, base, n_trad, n_juegos, version):
    raiz = ''
    cuerpo = f'''<main>
  <section class="heroe">
    <div class="heroe-t">
      <h1>Juega en español a juegos que nunca salieron traducidos</h1>
      <p class="lede">Más de {n_trad // 1000 * 1000:,} traducciones hechas por aficionados, de la NES a los juegos de PC de hoy. La Biblioteca de traducciones busca tus juegos, te dice cuáles tienen traducción y la aplica con un clic.</p>
      <div class="acciones">
        <a class="btn primario grande" href="{DESCARGA}">Descargar para Windows</a>
        <a class="btn grande" href="catalogo/">Buscar mi juego</a>
      </div>
      <p class="suave">Gratis y de código libre · Windows 10 y 11 · versión {e(version)}</p>
    </div>
    <img class="captura" src="img/captura-listas.png" width="1360" height="860" alt="La Biblioteca de traducciones con juegos de diez plataformas: Game Boy Advance, PlayStation, Super Nintendo, Mega CD, Dreamcast, Saturn, Nintendo DS, PlayStation 2, Nintendo 64 y PC">
  </section>

  <section class="cifras" aria-label="El catálogo en cifras">
    <div><b>{n_trad:,}</b><span>traducciones</span></div>
    <div><b>{n_juegos:,}</b><span>juegos</span></div>
    <div><b>{len(por_sis)}</b><span>sistemas</span></div>
    <div><b>0</b><span>juegos descargados: solo parches</span></div>
  </section>

  <section class="bloque">
    <h2>Cómo funciona</h2>
    <ol class="pasos grandes">
      <li><b>Dile dónde están tus juegos.</b> Busca ROMs sueltas o en .zip, .7z y .rar, discos .cue/.bin, .gdi y .chd, y los juegos de PC que tienes en Steam, GOG y Epic.</li>
      <li><b>Mira cuáles tienen traducción.</b> Por su CRC32 sabe si tu copia es la versión que pide cada traducción.</li>
      <li><b>Pulsa Traducir y luego Jugar.</b> Descarga el parche de su sitio original y lo aplica. El traducido queda junto al original con la etiqueta [T-Es]; tu copia no se toca.</li>
    </ol>
  </section>

  <section class="bloque doble">
    <div>
      <h2>Lo que hace por ti</h2>
      <ul class="lista-puntos">
        <li><b>Aplica los parches él solo:</b> IPS, BPS, UPS, PPF, APS y xdelta. Saca el juego de su comprimido o de su .chd y devuelve los discos traducidos en .chd.</li>
        <li><b>Sin descarga directa,</b> abre la página de la traducción y, en cuanto bajas el parche, lo aplica.</li>
        <li><b>Comprueba tu copia</b> con el CRC32 de la ficha o del léame de la traducción, y te avisa si es otra versión.</li>
        <li><b>Portadas, favoritos y listas</b> para apuntar lo que quieres jugar.</li>
        <li><b>Con RetroBat,</b> el botón Jugar abre el juego traducido con su emulador.</li>
        <li><b>Marca las traducciones hechas con IA</b> y te deja ocultarlas.</li>
      </ul>
    </div>
    <img class="captura" src="img/captura-ficha.png" width="1360" height="860" loading="lazy" alt="La ficha de Suikoden con sus tres traducciones y el menú para guardarlo en una lista">
  </section>

  <section class="bloque">
    <h2>Busca por sistema</h2>
    <div class="sistemas">{chips_sistemas(por_sis, sysn, raiz)}</div>
  </section>

  <section class="bloque">
    <h2>De dónde salen las traducciones</h2>
    <p class="texto">Las traducciones son de sus autores. El catálogo se ha hecho con los listados de romhacking.net, Traducciones Wave, Romhack Plaza, TraduSquare, Romhack Hispano, El Otro Lado, Steam Community, RetroAchievements, Parches en Castellano y blogs y repositorios de traductores. Cada ficha lleva a la página de su traducción, y el programa solo descarga los parches de los sitios que lo permiten.</p>
    <p class="texto">No se enlazan los sitios que reparten juegos ya parcheados o completos. Si eres autor de una traducción y quieres corregir su ficha o que no aparezca, <a href="{INCIDENCIAS}">abre una incidencia</a>.</p>
  </section>

  <section class="bloque preguntas">
    <h2>Preguntas</h2>
    <details><summary>¿Descarga juegos?</summary><p>No. Solo descarga los parches de traducción y los aplica sobre la copia que ya tienes.</p></details>
    <details><summary>¿Cuánto cuesta?</summary><p>Nada. Es gratuito y de código libre, con licencia GPL-3.0. Si te resulta útil, puedes <a href="{DONAR}">invitarme a un café</a>.</p></details>
    <details><summary>Windows dice «Windows protegió su PC». ¿Es seguro?</summary><p>El instalador no está firmado y por eso Windows avisa. Pulsa «Más información» y «Ejecutar de todas formas». El código está en <a href="{REPO}">GitHub</a> para quien quiera revisarlo.</p></details>
    <details><summary>¿Necesito RetroBat?</summary><p>No. Funciona solo y traduce los juegos que tengas en cualquier carpeta. Si usas RetroBat, el botón Jugar abre el juego traducido con su emulador.</p></details>
    <details><summary>¿Envía datos míos a algún sitio?</summary><p>No. Solo se conecta para traer portadas, descargar parches y ver si hay una versión o un catálogo nuevos. Tus ajustes y tus listas se quedan en tu equipo.</p></details>
    <details><summary>Falta una traducción o una ficha está mal</summary><p><a href="{INCIDENCIAS}">Avísame en GitHub</a>. El catálogo se pone al día solo en todos los equipos, sin reinstalar nada.</p></details>
  </section>

  <section class="final">
    <h2>¿Qué juego vas a jugar en español?</h2>
    <a class="btn primario grande" href="{DESCARGA}">Descargar para Windows</a>
  </section>
</main>'''
    desc = (f'Juega en español a juegos que nunca salieron traducidos: {n_trad:,} traducciones de aficionados, de la NES al PC. '
            'Programa gratuito para Windows que busca tus juegos y aplica la traducción con un clic.')
    return pagina(raiz, 'Biblioteca de traducciones: juega en español a juegos que nunca salieron traducidos', desc, '', cuerpo, base)


def pagina_404(base):
    cuerpo = f'''<main class="sistema">
  <h1>Esta página no existe</h1>
  <p class="lede">Puede que el juego haya cambiado de nombre en el catálogo. Búscalo en el <a href="{base}catalogo/">catálogo</a> o vuelve al <a href="{base}">inicio</a>.</p>
</main>'''
    return pagina(base, 'Página no encontrada · Biblioteca de traducciones', 'Esta página no existe.', '404.html', cuerpo, base)


def miles(t):
    """6,109 -> 6.109, sin tocar el resto del texto."""
    return re.sub(r'(?<=\d),(?=\d{3}\b)', '.', t)


def escribir(salida, ruta, texto):
    p = os.path.join(salida, *ruta.split('/'))
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, 'w', encoding='utf-8', newline='\n') as f:
        f.write(texto)


def main():
    salida = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(RAIZ, '_web'))
    cname = os.path.join(WEB, 'CNAME')
    dominio = open(cname, encoding='utf-8').read().strip() if os.path.isfile(cname) else ''
    base = f'https://{dominio}/' if dominio else 'https://alucardog-dotcom.github.io/biblioteca-traducciones/'
    version = json.load(open(os.path.join(RAIZ, 'src-tauri', 'tauri.conf.json'), encoding='utf-8'))['version']

    sysn = sistemas_app()
    filas, libretro = cargar()
    juegos = agrupar(filas)
    usados = set()
    por_sis = {}
    for g in sorted(juegos, key=lambda g: (g['s'], sin_acentos(g['titulo']).lower())):
        base_slug, n = slug(g['titulo']), 2
        ruta = f"juego/{g['s']}/{base_slug}/"
        while ruta in usados:
            ruta = f"juego/{g['s']}/{base_slug}-{n}/"
            n += 1
        usados.add(ruta)
        g['ruta'] = ruta
        por_sis.setdefault(g['s'], []).append(g)

    if os.path.isdir(salida):
        shutil.rmtree(salida)
    os.makedirs(salida)
    shutil.copytree(os.path.join(WEB, 'img'), os.path.join(salida, 'img'))
    for c in ('captura-listas.png', 'captura-ficha.png', 'captura-descubrir.png'):
        shutil.copy(os.path.join(RAIZ, 'docs', c), os.path.join(salida, 'img', c))
    shutil.copytree(os.path.join(RAIZ, 'ui', 'portadas'), os.path.join(salida, 'portadas'))
    shutil.copy(os.path.join(RAIZ, 'ui', 'icono.svg'), os.path.join(salida, 'icono.svg'))
    shutil.copy(os.path.join(WEB, 'estilo.css'), os.path.join(salida, 'estilo.css'))
    os.makedirs(os.path.join(salida, 'catalogo'))
    shutil.copy(os.path.join(WEB, 'catalogo.js'), os.path.join(salida, 'catalogo', 'catalogo.js'))
    if dominio:
        shutil.copy(cname, os.path.join(salida, 'CNAME'))
    escribir(salida, '.nojekyll', '')

    n_trad, n_juegos = len(filas), len(juegos)
    escribir(salida, 'index.html', miles(pagina_inicio(por_sis, sysn, base, n_trad, n_juegos, version)))
    escribir(salida, 'catalogo/index.html', miles(pagina_catalogo(por_sis, sysn, base, n_trad, n_juegos)))
    indice = []
    for g in juegos:
        escribir(salida, g['ruta'] + 'index.html', pagina_juego(g, sysn, libretro, base))
        busca = ' '.join(dict.fromkeys(sum((tokens(f['juego']) + tokens(f['titulo_original']) for f in g['filas']), [])
                                       + [sin_acentos(f['grupo']).lower() for f in g['filas'] if f['grupo']]))
        indice.append([g['titulo'], g['s'], g['ruta'], len(g['filas']), int(any(directa(f) for f in g['filas'])), busca])
    indice.sort(key=lambda j: sin_acentos(j[0]).lower())
    escribir(salida, 'catalogo/juegos.json', json.dumps(indice, ensure_ascii=False, separators=(',', ':')))
    for s, gs in por_sis.items():
        escribir(salida, f'sistema/{s}/index.html', miles(pagina_sistema(s, gs, sysn, base)))
    escribir(salida, '404.html', pagina_404(base))

    hoy = date.today().isoformat()
    rutas = ['', 'catalogo/'] + [f'sistema/{s}/' for s in por_sis] + [g['ruta'] for g in juegos]
    escribir(salida, 'sitemap.xml', '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
             + ''.join(f'<url><loc>{e(base + r)}</loc><lastmod>{hoy}</lastmod></url>\n' for r in rutas) + '</urlset>\n')
    escribir(salida, 'robots.txt', f'User-agent: *\nAllow: /\nSitemap: {base}sitemap.xml\n')
    print(f'Web en {salida}: {n_juegos} juegos, {len(por_sis)} sistemas, {len(rutas)} páginas. Dirección: {base}')


if __name__ == '__main__':
    main()
