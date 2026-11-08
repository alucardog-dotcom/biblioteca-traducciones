# Genera la web de la Biblioteca de traducciones (GitHub Pages) a partir del catálogo: la portada, el
# buscador, una página por juego y otra por sistema. Solo usa la biblioteca estándar de Python.
# Uso: python tools/web.py [carpeta de salida, _web por defecto]
# El dominio va en web/CNAME; si no está, la web queda en la dirección de GitHub Pages. Los ficheros de
# verificación de Google Search Console (google….html) y Bing (BingSiteAuth.xml) se dejan en web/ y van a la raíz.
import collections, csv, html, io, json, os, re, shutil, subprocess, sys, unicodedata
from datetime import date
from urllib.parse import quote

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CAT = os.path.join(RAIZ, 'src-tauri', 'recursos', 'catalogo')
WEB = os.path.join(RAIZ, 'web')
REPO = 'https://github.com/alucardog-dotcom/biblioteca-traducciones'
DESCARGA = REPO + '/releases/latest'
# El instalador de la última versión con nombre fijo: publicar.yml sube cada instalador también con ese nombre
# y GitHub resuelve «latest» a la última versión publicada, así que el botón descarga el .exe directamente.
INSTALADOR = REPO + '/releases/latest/download/Biblioteca-de-traducciones-setup.exe'
# Estadísticas anónimas y sin cookies con GoatCounter: el código del sitio (lo que va antes de .goatcounter.com).
# Vacío, la web no carga ningún script de estadísticas.
GOATCOUNTER = 'bibliotecadetraducciones'
INCIDENCIAS = REPO + '/issues'
DONAR = 'https://paypal.me/btraducciones'
# Enlace de afiliado: la búsqueda de Instant Gaming con el código del autor (igr).
INSTANT_GAMING = 'https://www.instant-gaming.com/es/busquedas/?query={}&igr=gamer-d73144'
LIBRETRO = {}  # nombres de los sistemas en thumbnails.libretro.com (de centro_datos.json); lo rellena main()
PC = {'dos', 'windows', 'scummvm', 'easyrpg', 'pc'}
# Consolas de hoy: sus traducciones son mods que se instalan a mano (como MODERNAS en ui/app.js).
MODERNAS = {'switch', 'ps3', 'ps4', 'ps5', 'wiiu', 'xbox360', 'xboxone'}
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


def web_segura(u):
    """Solo direcciones web en los href que vienen del catálogo (nunca javascript: ni file:)."""
    return u if u.startswith(('https://', 'http://')) and ' ' not in u else ''


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
        g = {'titulo': rep['juego'], 's': rep['sistema'], 'filas': sorted(fs, key=orden), 'cover': cover,
             'fecha': max((fecha_iso(f) for f in fs), default='')}
        for f in fs:
            f['gr'] = g
        out.append(g)
    return out


def fecha_iso(f):
    """La fecha de la traducción como AAAA-MM-DD, o '' si no está entera."""
    d = f['fecha'].strip()
    return d if re.fullmatch(r'\d{4}-\d{2}-\d{2}', d) else ''


def anio(f):
    m = re.match(r'\d{4}', f['fecha'].strip())
    return m.group(0) if m else ''


def es_ia(f):
    return f['notas'].startswith('[IA]')


def adulto(f):
    """Traducción de un juego para adultos (+18): la marca a18 de centro_datos (de cache\adultos.tsv)."""
    return bool(f['x'].get('a18'))


def juego_adulto(g):
    """Un juego se oculta cuando todas sus traducciones son de adultos."""
    return all(adulto(f) for f in g['filas'])


def banderas(g):
    """Lo que filtra el buscador (catalogo.js): 1 alguna completa, 2 alguna sin IA, 4 se vende en Steam, 8 para adultos."""
    return ((1 if any(f['estado'].strip().lower().startswith('complet') for f in g['filas']) else 0)
            | (2 if any(not es_ia(f) for f in g['filas']) else 0)
            | (4 if any(f['x'].get('steam') for f in g['filas']) else 0)
            | (8 if juego_adulto(g) else 0))


def directa(f):
    """Un clic: el programa la baja y la aplica. Las de PC y consolas de hoy se instalan a mano."""
    return bool(f['x'].get('d')) and f['sistema'] not in PC | MODERNAS


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


# ---------------------------------------------------------------- texto para quien lee (y para los buscadores)
FORMATOS = {'ips': 'parche IPS', 'bps': 'parche BPS', 'ups': 'parche UPS', 'ppf': 'parche PPF', 'aps': 'parche APS',
            'xdelta': 'parche xdelta', 'parche': 'parche', 'archivos': 'archivos que se copian sobre el juego', 'mod': 'mod',
            'instalador': 'instalador', 'workshop': 'mod del Workshop de Steam'}
ESTADOS = {'completa': 'está completa', 'publicada': 'está publicada', 'beta': 'está en beta', 'parcial': 'es parcial',
           'en curso': 'está en curso', 'en desarrollo': 'está en desarrollo', 'en actualización': 'está en actualización',
           'abandonada': 'quedó abandonada'}


def titulo_original(gr):
    t = gr['titulo']
    return next((f['titulo_original'].strip() for f in gr['filas']
                 if f['titulo_original'].strip() and f['titulo_original'].strip() != t), '')


def frase_trad(f):
    """Una frase con lo que se sabe de la traducción: quién, de cuándo, cómo está y cómo se aplica."""
    quien = f['grupo'].strip() or 'autor desconocido'
    datos = []
    if anio(f):
        datos.append(f'es de {anio(f)}')
    est, ver = ESTADOS.get(f['estado'].strip().lower()), f['version'].strip()
    if est:
        datos.append(est + (f' (versión {ver})' if ver else ''))
    elif ver:
        datos.append(f'va por la versión {ver}')
    fmt = FORMATOS.get(f['formato_parche'].strip().lower())
    roms = [r.strip() for r in f['rom_requerida'].split('|') if r.strip()]
    if fmt:
        datos.append(f'viene como {fmt}' + (f' para la versión {" o ".join(roms)}' if roms else ''))
    elif roms:
        datos.append(f'pide la versión {" o ".join(roms)}')
    frase = f'La traducción de {quien}' + (', hecha con IA o con traducción automática,' if es_ia(f) else '')
    if datos:
        frase += ' ' + (datos[0] if len(datos) == 1 else ', '.join(datos[:-1]) + ' y ' + datos[-1])
    else:
        frase += ' no tiene más datos en el catálogo'
    if directa(f):
        frase += '; la Biblioteca de traducciones la descarga y la aplica con un clic'
    return frase + '.'


def resumen_juego(gr, nombre_sis):
    """Dos párrafos propios de cada juego, con las palabras con las que la gente busca."""
    titulo, fs = gr['titulo'], gr['filas']
    orig, n = titulo_original(gr), len(fs)
    intro = (f'{titulo} es un juego de {nombre_sis}' + (f' (título original: {orig})' if orig else '') + '. '
             + ('Esta ficha reúne su traducción al español (castellano), hecha por aficionados, con sus datos y cómo aplicarla a tu copia.'
                if n == 1 else
                f'Esta ficha reúne sus {n} traducciones al español (castellano), hechas por aficionados, con sus datos y cómo aplicarlas a tu copia.'))
    frases = [frase_trad(f) for f in fs[:4]] + ([f'Y hay {n - 4} más, abajo.'] if n > 4 else [])
    return f'<p class="texto">{e(intro)}</p>\n    <p class="texto">{e(" ".join(frases))}</p>'


def vecinos(lista, g, k=3):
    """Los k juegos de antes y de después de g en una lista ordenada por título."""
    i = next((j for j, x in enumerate(lista) if x is g), -1)
    if i < 0:
        return [x for x in lista if x is not g][:2 * k]
    return lista[max(0, i - k):i] + lista[i + 1:i + 1 + k]


def item_juego(g, raiz, sysn, con_sistema=False):
    n = len(g['filas'])
    extra = ((f' <span class="n">{e(sysn.get(g["s"], g["s"]))}</span>' if con_sistema else '')
             + (f' <span class="n">{n} traducciones</span>' if n > 1 else ''))
    return f'<li><a href="{raiz}{g["ruta"]}">{e(g["titulo"])}</a>{extra}</li>'


def tarjeta_juego(g, raiz, sysn, con_sistema=True, fila=None, extra='', pie=''):
    """Un juego como en el programa: la portada (con las alternativas por si una falla), el título, el sistema
    y sus traducciones. El título queda debajo mientras la portada carga, y se queda si no hay ninguna."""
    urls = portadas(g, LIBRETRO, raiz)
    img = (f'<img loading="lazy" src="{e(urls[0])}" data-alt="{e(" ".join(urls[1:]))}" alt="" referrerpolicy="no-referrer" '
           f'onerror="sig(this)" onload="this.classList.add(\'ok\')">') if urls else ''
    n = len(g['filas'])
    f = fila or g['filas'][0]
    partes = [e(sysn.get(g['s'], g['s'])) if con_sistema else '',
              e(f['grupo'].strip()) if (fila or n == 1) and f['grupo'].strip() else '',
              f'{n} traducciones' if n > 1 else '',
              '<b>Un clic</b>' if any(directa(x) for x in g['filas']) else '']
    return (f'<li class="tarjeta" data-ruta="{e(g["ruta"])}" data-sis="{e(g["s"])}" data-sisn="{e(sysn.get(g["s"], g["s"]))}" data-titulo="{e(g["titulo"])}"{' data-a18="1"' if juego_adulto(g) else ''}>'
            f'<a href="{raiz}{g["ruta"]}"><span class="port{"" if img else " sin"}">{img}<span class="ph">{e(g["titulo"])}</span></span>'
            f'<span class="cuerpo"><span class="t">{e(g["titulo"])}</span><span class="m">{" · ".join(p for p in partes if p)}{extra}</span>{pie}</span></a><button type="button" class="fav" aria-pressed="false" aria-label="Añadir a Favoritos"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 20s-7.5-4.6-9.3-9.2C1.4 7.4 3.6 4 7 4c2 0 3.6 1.1 5 3 1.4-1.9 3-3 5-3 3.4 0 5.6 3.4 4.3 6.8C19.5 15.4 12 20 12 20z"/></svg></button></li>')


def portada_spec(g):
    """La portada para el buscador (catalogo.js), en corto: m:<fichero a mano>, s:<AppID>[|<pu>],
    l:<sistema libretro>|<tipo>|<nombre>[|<pu>] o u:<dirección>. catalogo.js hace con ello las mismas direcciones que portadas()."""
    x, s = g['cover']['x'], g['cover']['sistema']
    sis = x.get('ps') or LIBRETRO.get(s)
    pu = ('|' + x['pu']) if x.get('pu') else ''
    if x.get('pm'):
        return 'm:' + x['pm']
    if x.get('steam'):
        return 's:' + str(x['steam']) + pu
    if x.get('p') and sis:
        return 'l:' + sis + '|' + (x.get('pt') or 'Named_Boxarts') + '|' + x['p'] + pu
    return ('u:' + x['pu']) if x.get('pu') else ''


def relacionados(gr, ctx, raiz, nombre_sis):
    """Enlaces a los juegos vecinos del mismo sistema y a otras traducciones del mismo grupo."""
    sysn, bloques = ctx['sysn'], []
    cerca = vecinos(ctx['por_sis'][gr['s']], gr)
    if cerca:
        bloques.append(f'<h2>Más juegos de {e(nombre_sis)} en español</h2><ul class="tarjetas">{"".join(tarjeta_juego(g, raiz, sysn, False) for g in cerca)}</ul>'
                       f'<p class="suave"><a href="{raiz}sistema/{e(gr["s"])}/">Todos los juegos de {e(nombre_sis)} con traducción</a></p>')
    for grupo in dict.fromkeys(f['grupo'].strip() for f in gr['filas'] if f['grupo'].strip()):
        otros = vecinos(ctx['por_grupo'].get(grupo, []), gr)
        if otros:
            bloques.append(f'<h2>Más traducciones de {e(grupo)}</h2><ul class="tarjetas">{"".join(tarjeta_juego(g, raiz, sysn) for g in otros)}</ul>')
            break
    return ''.join(f'<section class="bloque relacionados">{b}</section>' for b in bloques)


def recientes(filas, raiz, sysn, k, con_sistema):
    """Las últimas traducciones con fecha entera, un juego por línea."""
    vistos, items = set(), []
    for f in sorted((f for f in filas if fecha_iso(f)), key=fecha_iso, reverse=True):
        g = f['gr']
        if id(g) in vistos:
            continue
        vistos.add(id(g))
        items.append(tarjeta_juego(g, raiz, sysn, con_sistema, fila=f, extra=f' · <span class="fecha">{fecha_iso(f)}</span>'))
        if len(items) == k:
            break
    return f'<ul class="tarjetas">{"".join(items)}</ul>' if items else ''


# ---------------------------------------------------------------- novedades
DIAS_NOVEDADES = 120  # cuántos días atrás se mira el historial del catálogo para las novedades
MESES = ['enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', 'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre']


def fecha_larga(d):
    """2026-11-08 -> 8 de noviembre de 2026."""
    a, m, dd = d.split('-')
    return f'{int(dd)} de {MESES[int(m) - 1]} de {a}'


def fecha_corta(d):
    """2026-11-08 -> 8 nov."""
    a, m, dd = d.split('-')
    return f'{int(dd)} {MESES[int(m) - 1][:3]}'


def clave_fila(f):
    return f"{f['sistema']}|{f['juego']}|{f['grupo']}"


def fechas_alta():
    """Cuándo entró cada traducción en el catálogo, según el historial de git del CSV: clave de la fila -> AAAA-MM-DD.
    Se miran los commits de los últimos DIAS_NOVEDADES días; las filas que ya estaban en el más antiguo de ellos no
    llevan fecha, porque no se sabe cuándo entraron. Sin git o sin historial (un clon superficial), no hay fechas."""
    ruta = 'src-tauri/recursos/catalogo/traducciones_fan.csv'

    def git(*args):
        return subprocess.run(['git', '-C', RAIZ, *args], capture_output=True, check=True).stdout

    try:
        commits = [l.split() for l in git('log', '--format=%H %cs', f'--since={DIAS_NOVEDADES} days ago', '-n', '500', '--', ruta)
                   .decode().splitlines() if l.strip()]
        if len(commits) < 2:
            return {}
        altas, viejas = {}, None
        for sha, fecha in reversed(commits):  # del más antiguo al más nuevo
            claves = {clave_fila(f) for f in csv.DictReader(io.StringIO(git('show', f'{sha}:{ruta}').decode('utf-8-sig')))}
            if viejas is None:
                viejas = claves
                continue
            for c in claves:
                if c not in viejas and c not in altas:
                    altas[c] = fecha
        return altas
    except (OSError, subprocess.CalledProcessError, UnicodeDecodeError, KeyError):
        return {}


def novedades(filas, sin_adultos=False):
    """Las filas con fecha de alta por días, del más nuevo al más antiguo: [(AAAA-MM-DD, [filas])]. Dentro de un día,
    primero las que tienen portada y no son de IA, y luego por la fecha de la traducción, para que la portada luzca."""
    dias = {}
    for f in filas:
        if f.get('alta') and not (sin_adultos and juego_adulto(f['gr'])):
            dias.setdefault(f['alta'], []).append(f)
    for fs in dias.values():
        fs.sort(key=fecha_iso, reverse=True)
        fs.sort(key=lambda f: (not con_portada(f), es_ia(f)))
    return sorted(dias.items(), reverse=True)


def tarjetas_novedades(dias, raiz, sysn, k=None, con_fecha=True):
    """Las tarjetas de las novedades, un juego por tarjeta con su traducción nueva, hasta k. Con k (la portada), el
    mismo título en dos sistemas sale una sola vez."""
    vistos, titulos, items = set(), set(), []
    for fecha, fs in dias:
        for f in fs:
            g = f['gr']
            if id(g) in vistos or (k and sin_acentos(g['titulo']).lower() in titulos):
                continue
            vistos.add(id(g))
            titulos.add(sin_acentos(g['titulo']).lower())
            pie = f'<span class="m alta">Añadido el {fecha_corta(fecha)}</span>' if con_fecha else ''
            items.append(tarjeta_juego(g, raiz, sysn, True, fila=f, pie=pie))
            if k and len(items) == k:
                return items
    return items


def jsonld(datos):
    return '<script type="application/ld+json">' + json.dumps(datos, ensure_ascii=False).replace('<', '\\u003c') + '</script>'


def migas_ld(base, pasos):
    """BreadcrumbList para los buscadores: [(nombre, ruta), ...], la última es la página actual."""
    return {'@type': 'BreadcrumbList', 'itemListElement': [
        {'@type': 'ListItem', 'position': i + 1, 'name': n, 'item': base + r} for i, (n, r) in enumerate(pasos)]}


# ---------------------------------------------------------------- páginas
def pagina(raiz, titulo, descripcion, ruta, cuerpo, base, imagen='img/vista-previa.png', tipo='website', ld=None):
    canon = base + ruta
    og_imagen = imagen if imagen.startswith('http') else base + imagen
    cuenta = (f'<script data-goatcounter="https://{GOATCOUNTER}.goatcounter.com/count" async src="//gc.zgo.at/count.js"></script>\n'
              if GOATCOUNTER else '')
    nota_cuenta = ' Estadísticas anónimas y sin cookies con <a href="https://www.goatcounter.com/">GoatCounter</a>.' if GOATCOUNTER else ''
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
<link rel="alternate" type="application/atom+xml" title="Novedades del catálogo" href="{raiz}novedades.xml">
<meta property="og:type" content="{tipo}">
<meta property="og:title" content="{e(titulo)}">
<meta property="og:description" content="{e(descripcion)}">
<meta property="og:url" content="{e(canon)}">
<meta property="og:image" content="{e(og_imagen)}">
<meta property="og:locale" content="es_ES">
<meta name="twitter:card" content="summary_large_image">
{jsonld(ld) if ld else ''}
<script>try{{if(localStorage.getItem('adultos')==='1')document.documentElement.classList.add('adultos')}}catch(e){{}}function sig(i){{var r=(i.dataset.alt||'').split(' ').filter(Boolean);if(!r.length){{i.parentNode.classList.add('sin');i.remove();return}}i.src=r.shift();i.dataset.alt=r.join(' ')}}</script>
</head>
<body>
<div class="franja" aria-hidden="true"></div>
<header class="barra">
  <a class="marca" href="{raiz or './'}"><img src="{raiz}icono.svg" alt="" width="32" height="32"><span>Biblioteca de traducciones</span></a>
  <nav aria-label="Principal">
    <a href="{raiz}catalogo/">Catálogo</a>
    <a href="{raiz}novedades/">Novedades</a>
    <a href="{raiz}mis-listas/" id="nav-listas">Mis listas</a>
    <a href="{REPO}">GitHub</a>
    <a class="btn primario" href="{INSTALADOR}" data-goatcounter-click="descarga-cabecera">Descargar</a>
  </nav>
</header>
{cuerpo}
<footer class="pie">
  <div class="pie-in">
    <p><b>Biblioteca de traducciones</b> es gratuita y de código libre (<a href="{REPO}/blob/main/LICENSE">GPL-3.0</a>). Las traducciones son de sus autores: este programa solo las encuentra y las aplica, y nunca descarga juegos.</p>
    <p><a href="{REPO}">Código en GitHub</a> · <a href="{INCIDENCIAS}">Avisar de un error o de una traducción que falta</a> · <a href="{DONAR}">Invítame a un café</a></p>
    <p class="suave">No está relacionado con ninguna de las empresas de los juegos. Las portadas son de sus propietarios. Firma del instalador: <a href="{REPO}#code-signing-policy">Code signing policy</a>.{nota_cuenta}</p>
  </div>
</footer>
<script src="{raiz}listas.js" defer></script>
{cuenta}</body>
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
              if web_segura(f['url']) else '<p class="suave">No se conoce la página de esta traducción.</p>')
    return f'''<article class="trad">
  <div class="trad-h"><h3>{e(f['grupo'] or 'Autor desconocido')}</h3><div class="chips">{chips_trad(f)}</div></div>
  {f'<dl>{dl}</dl>' if dl else ''}
  {f'<p class="nota">{e(nota)}</p>' if nota else ''}
  {enlace}
</article>'''


def pagina_juego(gr, sysn, libretro, base, ctx):
    raiz = '../../../'
    titulo, s = gr['titulo'], gr['s']
    nombre_sis = sysn.get(s, s)
    grupos = list(dict.fromkeys(f['grupo'] for f in gr['filas'] if f['grupo']))
    n = len(gr['filas'])
    urls = portadas(gr, libretro, raiz)
    img = (f'<img src="{e(urls[0])}" data-alt="{e(" ".join(urls[1:]))}" alt="Portada de {e(titulo)}" '
           f'referrerpolicy="no-referrer" onerror="sig(this)">') if urls else ''
    # Para las redes y los buscadores, la portada con dirección completa.
    absolutas = portadas(gr, libretro, base)
    imagen = absolutas[0] if absolutas else 'img/vista-previa.png'
    quien = ', '.join(grupos[:3]) + (' y otros' if len(grupos) > 3 else '')
    orig = titulo_original(gr)
    # Los que están en Steam (tienen AppID) se venden en las tiendas de claves; los gratuitos y los de
    # aficionados, no. Se busca por el nombre de la ficha (en los de Steam, el de la tienda; el título
    # original de las novelas visuales suele ir en japonés), sin lo que va entre paréntesis.
    en_steam = next((f for f in gr['filas'] if f['x'].get('steam')), None)
    compra = ''
    if en_steam:
        nombre = re.sub(r'\s*[(\[][^)\]]*[)\]]', '', en_steam['juego']).strip()
        compra = f"""<div class="compra">
        <a class="btn tienda" href="{e(INSTANT_GAMING.format(quote(nombre)))}" target="_blank" rel="sponsored noopener">¿No lo tienes? Búscalo en Instant Gaming</a>
        <p class="suave">Enlace de afiliado: si lo compras, me llevo una pequeña comisión sin que te cueste más.</p>
      </div>"""
    # Los que no están en Steam: la página oficial del juego ("pj", itch.io o la web de su autor), si se conoce.
    con_pagina = next((f for f in gr['filas'] if web_segura(f['x'].get('pj', ''))), None)
    if con_pagina and not en_steam:
        pj = con_pagina['x']['pj']
        sitio = ('itch.io' if '.itch.io/' in pj else 'Game Jolt' if 'gamejolt.com/' in pj else 'Steam' if 'store.steampowered.com/' in pj
                 else 'GOG' if '.gog.com/' in pj else 'la web oficial')
        compra = f"""<div class="compra">
        <a class="btn tienda" href="{e(pj)}" target="_blank" rel="noopener">¿No lo tienes? Página del juego en {sitio}{' (18+)' if con_pagina['x'].get('pj18') else ''}</a>
      </div>"""
    detalle = ''
    if n == 1:
        f0 = gr['filas'][0]
        bits = [b for b in (anio(f0), f0['estado'].strip().lower()) if b]
        detalle = f' ({", ".join(bits)})' if bits else ''
    desc = (f'{"Traducción" if n == 1 else f"{n} traducciones"} al español de {titulo} para {nombre_sis}'
            + (f', de {quien}' if quien else '') + detalle
            + ('. Se instala a mano; la Biblioteca de traducciones te lleva a su página.' if s in PC | MODERNAS
               else '. Aplícala sobre tu copia con un clic con la Biblioteca de traducciones.'))
    if s in MODERNAS:
        como = f"""<ol class="pasos">
      <li>Abre la página de la traducción y comprueba qué versión o actualización del juego pide.</li>
      <li>Copia el mod en la carpeta de mods de tu emulador (Ryujinx, RPCS3, shadPS4, Cemu, Xenia…) o de tu consola con firmware modificado, como explique el traductor.</li>
      <li>Con la Biblioteca de traducciones puedes buscar «{e(titulo)}» entre tus juegos y abrir su ficha y la página de la traducción.</li>
    </ol>
    <p class="suave">Necesitas tu propia copia del juego: ni la web ni el programa descargan juegos.</p>"""
    elif s in PC:
        como = """<ol class="pasos">
      <li>Descarga la Biblioteca de traducciones e instálala.</li>
      <li>Si tienes el juego en Steam, GOG o Epic, la biblioteca lo encuentra sola.</li>
      <li>Las traducciones de PC se instalan a mano: abre la ficha del juego y te lleva a la página de la traducción y a la carpeta del juego.</li>
    </ol>"""
    else:
        como = f"""<ol class="pasos">
      <li>Descarga la Biblioteca de traducciones e instálala.</li>
      <li>Dile en qué carpeta tienes tus juegos.</li>
      <li>Busca «{e(titulo)}» y pulsa <b>Traducir</b>. Comprueba que tu copia es la versión que pide la traducción y la aplica; tu original no se toca.</li>
    </ol>
    <p class="suave">Necesitas tu propia copia del juego: el programa no descarga juegos, solo los parches de traducción.</p>"""
    cuerpo = f"""<main class="juego">
  <nav class="migas" aria-label="Estás en"><a href="{raiz}">Inicio</a> › <a href="{raiz}catalogo/">Catálogo</a> › <a href="{raiz}sistema/{e(s)}/">{e(nombre_sis)}</a></nav>
  <div class="juego-h">
    <div class="portada{'' if img else ' sin'}">{img}<span class="ph">{e(titulo)}</span></div>
    <div class="juego-t">
      <p class="eti">{e(nombre_sis)}{' <span class="chip aviso" title="Juego para adultos">+18</span>' if juego_adulto(gr) else ''}</p>
      <h1>{e(titulo)} <span>en español</span></h1>
      {f'<p class="orig">Título original: {e(orig)}</p>' if orig else ''}
      <p class="lede">{'Hay una traducción al español hecha por aficionados' if n == 1 else f'Hay {n} traducciones al español hechas por aficionados'}{f": {e(quien)}" if quien else ''}.</p>
      <a class="btn primario grande" href="{INSTALADOR}" data-goatcounter-click="descarga-juego">Descargar el programa para Windows</a>
      {compra}
      <div class="juego-acciones" data-ruta="{e(gr["ruta"])}" data-sis="{e(s)}" data-sisn="{e(nombre_sis)}" data-titulo="{e(titulo)}" data-port="{e(" ".join(absolutas[:3]))}"><button type="button" class="fav" aria-pressed="false" aria-label="Añadir a Favoritos"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 20s-7.5-4.6-9.3-9.2C1.4 7.4 3.6 4 7 4c2 0 3.6 1.1 5 3 1.4-1.9 3-3 5-3 3.4 0 5.6 3.4 4.3 6.8C19.5 15.4 12 20 12 20z"/></svg></button><button type="button" class="btn" id="guardar-lista">Guardar en una lista</button></div>
    </div>
  </div>
  <section class="bloque">
    <h2>{'La traducción' if n == 1 else 'Las traducciones'}</h2>
    <div class="trads">{''.join(tarjeta_trad(f) for f in gr['filas'])}</div>
  </section>
  <section class="bloque">
    <h2>En resumen</h2>
    {resumen_juego(gr, nombre_sis)}
  </section>
  <section class="bloque como">
    <h2>Cómo jugarlo en español</h2>
    {como}
  </section>
  {relacionados(gr, ctx, raiz, nombre_sis)}
  <p class="suave">¿Algún dato está mal o eres el autor y quieres cambiar algo? <a href="{INCIDENCIAS}">Avísame en GitHub</a>.</p>
</main>"""
    titulo_pagina = f'{titulo} ({nombre_sis}): traducción al español · Biblioteca de traducciones'
    canon = base + gr['ruta']
    juego = {'@type': 'VideoGame', 'name': titulo, 'gamePlatform': nombre_sis, 'url': canon}
    if orig:
        juego['alternateName'] = orig
    if imagen.startswith('http'):
        juego['image'] = imagen
    ld = {'@context': 'https://schema.org', '@graph': [
        migas_ld(base, [('Inicio', ''), ('Catálogo', 'catalogo/'), (nombre_sis, f'sistema/{s}/'), (titulo, gr['ruta'])]),
        {'@type': 'WebPage', '@id': canon, 'url': canon, 'name': titulo_pagina, 'description': desc, 'inLanguage': 'es', 'about': juego}]}
    return pagina(raiz, titulo_pagina, desc, gr['ruta'], cuerpo, base, imagen=imagen, tipo='article', ld=ld)


def pagina_sistema(s, juegos, sysn, base):
    raiz = '../../'
    nombre = sysn.get(s, s)
    n = sum(len(g['filas']) for g in juegos)
    letras = {}
    for g in sorted(juegos, key=lambda g: sin_acentos(g['titulo']).lower()):
        l = sin_acentos(g['titulo'])[:1].upper()
        letras.setdefault(l if l.isalpha() else '#', []).append(g)

    bloques = ''.join(f'<section class="letra"><h2>{e(l)}</h2><ul class="tarjetas">{"".join(tarjeta_juego(g, raiz, sysn, False) for g in gs)}</ul></section>'
                      for l, gs in letras.items())
    filas = [f for g in juegos for f in g['filas']]
    grupos = collections.Counter(f['grupo'].strip() for f in filas if f['grupo'].strip())
    top = [g for g, _ in grupos.most_common(5)]
    clic = sum(1 for f in filas if directa(f))
    texto = (f'{nombre} tiene {n:,} {"traducción" if n == 1 else "traducciones"} al español de {len(juegos):,} {"juego" if len(juegos) == 1 else "juegos"}, '
             + ('hecha' if n == 1 else 'hechas') + ' por aficionados'
             + (f': entre otros, {", ".join(top[:-1])} y {top[-1]}' if len(top) > 1 else f': {top[0]}' if top else '') + '. '
             + (f'{clic:,} de ellas se aplican con un clic desde la Biblioteca de traducciones, que comprueba que tu copia es la versión que pide cada parche.'
                if clic > 1 else
                'La Biblioteca de traducciones te dice cuáles de tus juegos tienen traducción y te lleva a la página de cada una para instalarla.'))
    ultimas = recientes(filas, raiz, sysn, 8, False)
    cuerpo = f'''<main class="sistema">
  <nav class="migas" aria-label="Estás en"><a href="{raiz}">Inicio</a> › <a href="{raiz}catalogo/">Catálogo</a></nav>
  <h1>Traducciones al español de {e(nombre)}</h1>
  <p class="lede">{n:,} {"traducción" if n == 1 else "traducciones"} de {len(juegos):,} {"juego" if len(juegos) == 1 else "juegos"}, hechas por aficionados. Elige un juego para ver sus traducciones.</p>
  <p class="texto">{e(texto)}</p>
  {f'<section class="bloque"><h2>Últimas traducciones de {e(nombre)}</h2>{ultimas}</section>' if ultimas else ''}
  <section class="bloque"><h2>Todos los juegos</h2></section>
  {bloques}
</main>'''
    ld = {'@context': 'https://schema.org', '@graph': [
        migas_ld(base, [('Inicio', ''), ('Catálogo', 'catalogo/'), (nombre, f'sistema/{s}/')]),
        {'@type': 'CollectionPage', 'url': base + f'sistema/{s}/', 'name': f'Juegos de {nombre} en español', 'inLanguage': 'es'}]}
    desc = (f'{n:,} traducciones al español de juegos de {nombre} hechas por aficionados, '
            + ('reunidas en la Biblioteca de traducciones.' if s in PC | MODERNAS
               else 'listas para aplicar con un clic con la Biblioteca de traducciones.'))
    return pagina(raiz, f'Juegos de {nombre} en español: traducciones · Biblioteca de traducciones', desc, f'sistema/{s}/', cuerpo, base, ld=ld)


def chips_sistemas(por_sis, sysn, raiz):
    return ''.join(f'<a class="sis" href="{raiz}sistema/{e(s)}/">{e(sysn.get(s, s))} <span>{len(sum((g["filas"] for g in gs), []))}</span></a>'
                   for s, gs in sorted(por_sis.items(), key=lambda kv: -sum(len(g['filas']) for g in kv[1])))


def pagina_catalogo(por_sis, sysn, base, n_trad, n_juegos, filas):
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
  <div class="filtros" role="group" aria-label="Filtros">
    <button type="button" class="chip" data-f="directa" aria-pressed="false" title="Traducciones que el programa o la web aplican solos">Un clic</button>
    <button type="button" class="chip" data-f="completa" aria-pressed="false">Completas</button>
    <button type="button" class="chip" data-f="sinia" aria-pressed="false" title="Sin traducciones hechas con IA">Sin IA</button>
    <button type="button" class="chip" data-f="steam" aria-pressed="false" title="Solo los juegos que se venden en Steam">En Steam</button>
    <label class="chip-l"><input type="checkbox" id="adultos"> Mostrar juegos para adultos (+18)</label>
  </div>
  <p class="suave" id="cuenta" aria-live="polite"></p>
  <ul class="tarjetas" id="res"></ul>
  <button class="btn" id="mas" hidden>Ver más</button>
  <section class="bloque">
    <h2>Por sistema</h2>
    <div class="sistemas">{chips_sistemas(por_sis, sysn, raiz)}</div>
  </section>
  <section class="bloque">
    <h2>Últimas traducciones</h2>
    {recientes(filas, raiz, sysn, 12, True)}
  </section>
</main>
<script>var SYS = {json.dumps(sysn, ensure_ascii=False).replace('<', '\\u003c')};</script>
<script src="catalogo.js" defer></script>'''
    desc = f'Busca entre {n_trad:,} traducciones al español de juegos hechas por aficionados, de la NES a la Switch y los juegos de PC, y aplícalas con un clic.'
    return pagina(raiz, 'Catálogo de traducciones al español · Biblioteca de traducciones', desc, 'catalogo/', cuerpo, base)


def pagina_inicio(por_sis, sysn, base, n_trad, n_juegos, version, dias, filas):
    """La portada: el buscador, las últimas novedades del catálogo (o, sin historial de git, las últimas traducciones
    por su fecha), las cifras, los sistemas y, ya después, el programa para Windows."""
    raiz = ''
    if dias:
        bloque_nov = f'''<section class="bloque novedades">
    <div class="cab"><h2>Últimas novedades</h2><p class="suave"><a href="novedades/">Todas las novedades</a> · <a href="novedades.xml">Canal RSS</a></p></div>
    <p class="texto suave">Lo último que ha entrado en el catálogo. Cada tarjeta lleva a la ficha del juego y de ahí a la traducción.</p>
    <ul class="tarjetas">{''.join(tarjetas_novedades(dias, raiz, sysn, 12))}</ul>
  </section>'''
    else:
        bloque_nov = f'''<section class="bloque novedades">
    <div class="cab"><h2>Últimas traducciones</h2><p class="suave"><a href="catalogo/">Todo el catálogo</a></p></div>
    {recientes(filas, raiz, sysn, 12, True)}
  </section>'''
    cuerpo = f'''<main>
  <section class="heroe uno">
    <div class="heroe-t">
      <h1>Juega en español a juegos que nunca salieron traducidos</h1>
      <p class="lede">{n_trad:,} traducciones hechas por aficionados, de la NES a la Switch y los juegos de PC de hoy, reunidas en un solo catálogo. Cada ficha te lleva a la traducción y te dice qué copia del juego pide; muchas se aplican en el navegador o con el programa en un clic.</p>
      <form class="buscador" role="search" action="catalogo/" method="get">
        <label class="sr" for="q">Buscar un juego</label>
        <input id="q" name="q" type="search" placeholder="¿Qué juego quieres jugar en español?" autocomplete="off">
        <button class="btn primario grande" type="submit">Buscar</button>
      </form>
      <div class="acciones">
        <a class="btn" href="catalogo/">Todo el catálogo</a>
        <a class="btn" href="parchear/">Parchear en el navegador</a>
        <a class="btn" href="{INSTALADOR}" data-goatcounter-click="descarga-portada">Programa para Windows</a>
      </div>
    </div>
  </section>

  <div id="favoritos-inicio"></div>

  {bloque_nov}

  <section class="cifras" aria-label="El catálogo en cifras">
    <div><b>{n_trad:,}</b><span>traducciones</span></div>
    <div><b>{n_juegos:,}</b><span>juegos</span></div>
    <div><b>{len(por_sis)}</b><span>sistemas</span></div>
    <div><b>0</b><span>juegos descargados: solo parches</span></div>
  </section>

  <section class="bloque">
    <h2>Busca por sistema</h2>
    <div class="sistemas">{chips_sistemas(por_sis, sysn, raiz)}</div>
  </section>

  <section class="bloque doble programa">
    <div>
      <p class="eti">El programa para Windows</p>
      <h2>Si prefieres que lo haga todo él</h2>
      <p class="texto">Dile dónde están tus juegos: ROMs sueltas o en .zip, .7z y .rar, discos .cue/.bin, .gdi y .chd, y los de PC que tienes en Steam, GOG y Epic. Te dice cuáles tienen traducción comprobando el CRC32 de tu copia, descarga el parche de su sitio original y lo aplica. El traducido queda junto al original con la etiqueta [T-Es]; tu copia no se toca.</p>
      <ul class="lista-puntos">
        <li><b>Parches IPS, BPS, UPS, PPF, APS y xdelta,</b> también dentro de comprimidos y de .chd, con los discos traducidos de vuelta en .chd.</li>
        <li><b>Portadas, favoritos y listas</b> para apuntar lo que quieres jugar, y aviso de las traducciones hechas con IA, que puedes ocultar.</li>
        <li><b>Con RetroBat,</b> el botón Jugar abre el juego traducido con su emulador.</li>
      </ul>
      <div class="acciones">
        <a class="btn primario grande" href="{INSTALADOR}" data-goatcounter-click="descarga-programa">Descargar para Windows</a>
        <a class="btn grande" href="{DESCARGA}">Todas las versiones</a>
      </div>
      <p class="suave">Gratis y de código libre · Windows 10 y 11 · versión {e(version)}</p>
    </div>
    <img class="captura" src="img/captura-listas.png" width="1360" height="860" loading="lazy" alt="La Biblioteca de traducciones con juegos de diez plataformas: Game Boy Advance, PlayStation, Super Nintendo, Mega CD, Dreamcast, Saturn, Nintendo DS, PlayStation 2, Nintendo 64 y PC">
  </section>

  <section class="bloque">
    <h2>De dónde salen las traducciones</h2>
    <p class="texto">Las traducciones son de sus autores. El catálogo se ha hecho con los listados de romhacking.net, Traducciones Wave, Romhack Plaza, TraduSquare, Romhack Hispano, El Otro Lado, Steam Community, RetroAchievements, Parches en Castellano y blogs y repositorios de traductores. Cada ficha lleva a la página de su traducción, y la web y el programa solo descargan los parches de los sitios que lo permiten.</p>
    <p class="texto">No se enlazan los sitios que reparten juegos ya parcheados o completos. Si eres autor de una traducción y quieres corregir su ficha o que no aparezca, <a href="{INCIDENCIAS}">abre una incidencia</a>.</p>
  </section>

  <section class="bloque preguntas">
    <h2>Preguntas</h2>
    <details><summary>¿Hace falta instalar algo?</summary><p>No. La web busca en el mismo catálogo que el programa, cada ficha enlaza a la traducción y la página <a href="parchear/">Parchear</a> aplica los parches en el navegador sin subir nada a ningún sitio. El programa para Windows añade buscar tus juegos él solo, comprobar tu copia por su CRC32, portadas y listas, y el botón Jugar con RetroBat.</p></details>
    <details><summary>¿Descarga juegos?</summary><p>No. Ni la web ni el programa descargan juegos: solo los parches de traducción, que se aplican sobre la copia que ya tienes.</p></details>
    <details><summary>¿Cuánto cuesta?</summary><p>Nada. Es gratuito y de código libre, con licencia GPL-3.0. Si te resulta útil, puedes <a href="{DONAR}">invitarme a un café</a>.</p></details>
    <details><summary>Falta una traducción o una ficha está mal</summary><p><a href="{INCIDENCIAS}">Avísame en GitHub</a>. El catálogo se pone al día en la web y en el programa sin reinstalar nada.</p></details>
    <details><summary>Windows dice «Windows protegió su PC». ¿Es seguro?</summary><p>El instalador no está firmado y por eso Windows avisa. Pulsa «Más información» y «Ejecutar de todas formas». El código está en <a href="{REPO}">GitHub</a> para quien quiera revisarlo.</p></details>
    <details><summary>¿Necesito RetroBat?</summary><p>No. El programa funciona solo y traduce los juegos que tengas en cualquier carpeta. Si usas RetroBat, el botón Jugar abre el juego traducido con su emulador.</p></details>
    <details><summary>¿Envía datos míos a algún sitio?</summary><p>No. La web solo pide las portadas y los parches; el programa, además, mira si hay una versión o un catálogo nuevos. Tus ajustes y tus listas se quedan en tu equipo.</p></details>
  </section>

  <section class="final">
    <h2>¿Qué juego vas a jugar en español?</h2>
    <div class="acciones">
      <a class="btn primario grande" href="catalogo/">Buscar mi juego</a>
      <a class="btn grande" href="{INSTALADOR}" data-goatcounter-click="descarga-final">Descargar para Windows</a>
    </div>
  </section>
</main>'''
    desc = (f'{n_trad:,} traducciones al español de juegos hechas por aficionados, de la NES a la Switch y el PC, con las novedades de cada día. '
            'Cada ficha lleva a la traducción; aplícala en el navegador o con el programa gratuito para Windows.')
    ld = {'@context': 'https://schema.org', '@graph': [
        {'@type': 'WebSite', 'url': base, 'name': 'Biblioteca de traducciones', 'inLanguage': 'es',
         'potentialAction': {'@type': 'SearchAction', 'target': {'@type': 'EntryPoint', 'urlTemplate': base + 'catalogo/?q={q}'},
                             'query-input': 'required name=q'}},
        {'@type': 'SoftwareApplication', 'name': 'Biblioteca de traducciones', 'url': base, 'description': desc, 'inLanguage': 'es',
         'operatingSystem': 'Windows 10, Windows 11', 'applicationCategory': 'UtilitiesApplication', 'softwareVersion': version,
         'downloadUrl': INSTALADOR, 'license': 'https://www.gnu.org/licenses/gpl-3.0.html', 'image': base + 'img/vista-previa.png',
         'offers': {'@type': 'Offer', 'price': '0', 'priceCurrency': 'EUR'}}]}
    return pagina(raiz, 'Biblioteca de traducciones: juega en español a juegos que nunca salieron traducidos', desc, '', cuerpo, base, ld=ld)


def pagina_novedades(dias, filas, sysn, base):
    """Lo que ha entrado en el catálogo, por días (los juegos para adultos van ocultos, como en el catálogo)."""
    raiz = '../'
    if dias:
        n = sum(len(fs) for _, fs in dias)
        bloques = ''.join(
            f'<section class="letra" id="d{fecha}"><h2>{fecha_larga(fecha)} <span class="n">{len(fs)} {"traducción" if len(fs) == 1 else "traducciones"}</span></h2>'
            f'<ul class="tarjetas">{"".join(tarjetas_novedades([(fecha, fs)], raiz, sysn, con_fecha=False))}</ul></section>\n'
            for fecha, fs in dias)
        intro = (f'Las {n:,} traducciones que han entrado en el catálogo en los últimos {DIAS_NOVEDADES} días, de la más reciente a la más antigua. '
                 'Cada tarjeta lleva a la ficha del juego y de ahí a la traducción.')
    else:
        bloques = f'<section class="bloque"><h2>Últimas traducciones</h2>{recientes(filas, raiz, sysn, 60, True)}</section>'
        intro = 'Las últimas traducciones publicadas, por su fecha.'
    cuerpo = f'''<main class="sistema novedades-p">
  <p class="migas"><a href="{raiz}">Inicio</a> › Novedades</p>
  <h1>Novedades del catálogo</h1>
  <p class="lede">{intro} También como <a href="{raiz}novedades.xml">canal RSS</a>.</p>
  {bloques}
</main>'''
    desc = 'Las últimas traducciones al español añadidas al catálogo de la Biblioteca de traducciones, día a día.'
    ld = {'@context': 'https://schema.org', '@graph': [migas_ld(base, [('Inicio', ''), ('Novedades', 'novedades/')])]}
    return pagina(raiz, 'Novedades: últimas traducciones añadidas · Biblioteca de traducciones', desc, 'novedades/', cuerpo, base, ld=ld)


def feed_novedades(dias, sysn, base):
    """Canal Atom de las novedades: una entrada por día con sus juegos (sin los de adultos)."""
    entradas = []
    for fecha, fs in dias[:30]:
        lista = ''.join(f'<li><a href="{e(base + f["ruta"])}">{e(f["juego"])}</a> ({e(sysn.get(f["sistema"], f["sistema"]))})'
                        + (f' · {e(f["grupo"].strip())}' if f['grupo'].strip() else '') + '</li>' for f in fs)
        n = len(fs)
        entradas.append(f'<entry><title>{e(fecha_larga(fecha))}: {n} {"traducción nueva" if n == 1 else "traducciones nuevas"}</title>'
                        f'<link href="{e(base)}novedades/#d{fecha}"/><id>{e(base)}novedades/#d{fecha}</id><updated>{fecha}T00:00:00Z</updated>'
                        f'<content type="html">{e("<ul>" + lista + "</ul>")}</content></entry>\n')
    actualizado = (dias[0][0] if dias else date.today().isoformat()) + 'T00:00:00Z'
    return ('<?xml version="1.0" encoding="utf-8"?>\n<feed xmlns="http://www.w3.org/2005/Atom">\n'
            f'<title>Novedades del catálogo · Biblioteca de traducciones</title><link href="{e(base)}novedades/"/>'
            f'<link rel="self" href="{e(base)}novedades.xml"/><id>{e(base)}novedades.xml</id><updated>{actualizado}</updated>'
            '<author><name>Biblioteca de traducciones</name></author>\n' + ''.join(entradas) + '</feed>\n')


def pagina_listas(base):
    raiz = '../'
    cuerpo = f'''<main class="listas">
  <p class="migas"><a href="{raiz}">Inicio</a> › Mis listas</p>
  <h1>Mis listas</h1>
  <p class="lede">Tus favoritos y las listas que hagas, guardados en este navegador. Pulsa el corazón de cualquier juego para añadirlo a Favoritos; en su ficha, «Guardar en una lista» para ponerlo donde quieras.</p>
  <div id="listas"></div>
</main>'''
    return pagina(raiz, 'Mis listas · Biblioteca de traducciones', 'Tus juegos favoritos y tus listas de traducciones, guardados en tu navegador.', 'mis-listas/', cuerpo, base)


def pagina_404(base):
    cuerpo = f'''<main class="sistema">
  <h1>Esta página no existe</h1>
  <p class="lede">Puede que el juego haya cambiado de nombre en el catálogo. Búscalo en el <a href="{base}catalogo/">catálogo</a> o vuelve al <a href="{base}">inicio</a>.</p>
</main>'''
    return pagina(base, 'Página no encontrada · Biblioteca de traducciones', 'Esta página no existe.', '404.html', cuerpo, base)


def parches_json(filas):
    """Lo que necesita «Parchear en el navegador» (web/parchear.js): cada traducción con sus CRC32 (los de la
    ficha y los de sus descargas) y de dónde se baja el parche, como lo hace el programa."""
    out = []
    for f in filas:
        x = f['x']
        crcs = {c.strip().upper() for c in f['crc32'].split('|') if re.fullmatch(r'[0-9a-fA-F]{8}', c.strip())}
        for d in x.get('d') or []:
            crcs.update(c.upper() for c in d.get('c') or [] if re.fullmatch(r'[0-9a-fA-F]{8}', c))
        if not crcs:
            continue
        descargas = []
        for d in x.get('d') or []:
            if d.get('u'):
                descargas.append({'t': d['t'], 'u': d['u'], 'f': d.get('f') or []})
            elif d.get('t') == 'github' and d.get('r'):
                descargas.append({'t': 'github', 'r': d['r'], 'tag': d.get('tag', '')})
        fila = {'k': f"{f['sistema']}|{f['juego']}|{f['grupo']}", 's': f['sistema'], 'j': f['juego'], 'g': f['grupo'], 'v': f['version'],
                'e': f['estado'], 'f': f['formato_parche'], 'c': sorted(crcs), 'd': descargas, 'u': web_segura(f['url']), 'p': f.get('ruta', ''),
                'r': f['rom_requerida']}
        if es_ia(f):
            fila['ia'] = 1
        if adulto(f):
            fila['a18'] = 1
        if f['notas'].strip():
            fila['n'] = f['notas'].strip()[:200]
        out.append(fila)
    return json.dumps(out, ensure_ascii=False, separators=(',', ':'))


def pagina_parchear(base, sysn):
    raiz = '../'
    cuerpo = f'''<main>
<p class="migas"><a href="{raiz}">Inicio</a> › Parchear en el navegador</p>
<h1>Parchear en el navegador</h1>
<p class="lede">Elige tu ROM o toda tu carpeta de juegos: la página reconoce cada juego por su CRC32, baja el parche de la web de sus autores y te da el juego traducido. Todo ocurre en tu navegador: tus juegos no se suben a ningún sitio.</p>
<div class="zona" id="zona">
  <input type="file" id="rom" aria-label="Tu ROM">
  <p>Arrastra aquí tu ROM (o un zip con ella) o <label for="rom" class="btn">elige el fichero</label></p>
</div>
<div class="zona carpeta">
  <input type="file" id="carpeta" webkitdirectory multiple aria-label="Tu carpeta de juegos">
  <p>O busca en toda una carpeta, como hace el programa: <button type="button" class="btn" id="elegir-carpeta">elegir carpeta</button></p>
  <p class="muted">Lee ROMs sueltas o en zip, y discos .cue y .gdi; el sistema lo saca de la extensión o del nombre de la subcarpeta (snes, psx, megadrive…). Nada sale de tu equipo. En Chrome y Edge la carpeta se recuerda para la próxima visita, y los CRC ya calculados también.</p>
  <div id="carpetas"></div>
</div>
<div id="mios"></div>
<div id="juego"></div>
<div id="trads"></div>
<div id="resultado"></div>
<details class="registro"><summary>Registro</summary><pre id="registro"></pre></details>
<p class="muted">Funciona mejor en Chrome y Edge. Los parches xdelta y los juegos en 7z, rar o CHD necesitan el <a href="{raiz}#descargar">programa</a>. Nunca descarga juegos: solo el parche de traducción, que se aplica sobre tu propia copia.</p>
</main>
<style>
.zona {{ margin-top: 28px; padding: 36px 24px; border: 2px dashed var(--linea); border-radius: 16px; background: var(--panel); text-align: center; }}
.zona.sobre {{ border-color: var(--amarillo); }}
.zona input {{ position: absolute; width: 1px; height: 1px; opacity: 0; }}
.zona p {{ margin: 0; }}
.zona.carpeta {{ margin-top: 14px; padding: 22px 24px; }}
.zona.carpeta .muted {{ margin-top: 8px; font-size: 0.9rem; }}
.mios-h {{ display: flex; flex-wrap: wrap; align-items: baseline; gap: 8px 18px; margin-top: 36px; }}
#mios h3 {{ margin-top: 28px; }}
.progreso {{ color: var(--suave); }}
.carpeta-g {{ white-space: nowrap; }}
.enlace {{ background: none; border: 0; color: var(--amarillo-t); cursor: pointer; font: inherit; padding: 0; text-decoration: underline; text-underline-offset: 3px; }}
.sis {{ font-size: 0.6em; color: var(--suave); font-weight: 600; margin-left: 10px; }}
.trad {{ margin: 14px 0; padding: 16px 18px; border: 1px solid var(--linea); border-radius: 12px; background: var(--panel); }}
.trad-t {{ display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }}
.trad-a {{ display: flex; flex-wrap: wrap; gap: 10px; margin-top: 12px; }}
.manual {{ display: inline-block; margin-top: 8px; color: var(--suave); }}
.hecho {{ margin: 22px 0; padding: 20px; border: 1px solid rgba(95, 211, 154, 0.4); border-radius: 12px; background: var(--panel); }}
.aviso {{ padding: 14px 16px; border: 1px solid rgba(255, 196, 0, 0.4); border-radius: 12px; background: var(--panel); }}
.registro {{ margin-top: 28px; color: var(--suave); }}
.registro pre {{ white-space: pre-wrap; font-size: 0.85rem; max-height: 240px; overflow: auto; background: #0B0E13; padding: 12px; border-radius: 8px; }}
#juego h2 {{ margin-top: 28px; }}
</style>
<script>var SYS = {json.dumps(sysn, ensure_ascii=False).replace('<', '\\u003c')};</script>
<script src="motor.js"></script>
<script src="parchear.js" defer></script>'''
    desc = 'Aplica una traducción al español a tu ROM desde el navegador: la página reconoce el juego por su CRC32, baja el parche de sus autores y te da el juego traducido. Nada se sube a ningún sitio.'
    return pagina(raiz, 'Parchear en el navegador · Biblioteca de traducciones', desc, 'parchear/', cuerpo, base)


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
    LIBRETRO.update(libretro)
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
        for f in g['filas']:
            f['ruta'] = ruta
        por_sis.setdefault(g['s'], []).append(g)
    por_grupo = {}
    for g in sorted(juegos, key=lambda g: sin_acentos(g['titulo']).lower()):
        for grupo in dict.fromkeys(f['grupo'].strip() for f in g['filas'] if f['grupo'].strip()):
            por_grupo.setdefault(grupo, []).append(g)
    ctx = {'sysn': sysn, 'por_sis': por_sis, 'por_grupo': por_grupo}
    altas = fechas_alta()
    for f in filas:
        f['alta'] = altas.get(clave_fila(f), '')
    dias, dias_publicas = novedades(filas), novedades(filas, sin_adultos=True)

    if os.path.isdir(salida):
        shutil.rmtree(salida)
    os.makedirs(salida)
    shutil.copytree(os.path.join(WEB, 'img'), os.path.join(salida, 'img'))
    for c in ('captura-listas.png', 'captura-ficha.png', 'captura-descubrir.png'):
        shutil.copy(os.path.join(RAIZ, 'docs', c), os.path.join(salida, 'img', c))
    shutil.copytree(os.path.join(RAIZ, 'ui', 'portadas'), os.path.join(salida, 'portadas'))
    # Copias reducidas de las portadas de internet que pesan mucho (las hace miniaturas.py); el catálogo
    # apunta a ellas en "pu", también para el programa.
    if os.path.isdir(os.path.join(WEB, 'miniaturas')):
        shutil.copytree(os.path.join(WEB, 'miniaturas'), os.path.join(salida, 'miniaturas'))
    shutil.copy(os.path.join(RAIZ, 'ui', 'icono.svg'), os.path.join(salida, 'icono.svg'))
    shutil.copy(os.path.join(WEB, 'estilo.css'), os.path.join(salida, 'estilo.css'))
    os.makedirs(os.path.join(salida, 'catalogo'))
    shutil.copy(os.path.join(WEB, 'catalogo.js'), os.path.join(salida, 'catalogo', 'catalogo.js'))
    os.makedirs(os.path.join(salida, 'parchear'))
    for nombre in ('motor.js', 'parchear.js'):
        shutil.copy(os.path.join(WEB, nombre), os.path.join(salida, 'parchear', nombre))
    shutil.copy(os.path.join(WEB, 'listas.js'), os.path.join(salida, 'listas.js'))
    if dominio:
        shutil.copy(cname, os.path.join(salida, 'CNAME'))
    for nombre in os.listdir(WEB):
        if re.fullmatch(r'google[0-9a-f]+\.html|BingSiteAuth\.xml', nombre):
            shutil.copy(os.path.join(WEB, nombre), os.path.join(salida, nombre))
    escribir(salida, '.nojekyll', '')

    n_trad, n_juegos = len(filas), len(juegos)
    escribir(salida, 'index.html', miles(pagina_inicio(por_sis, sysn, base, n_trad, n_juegos, version, dias_publicas, filas)))
    escribir(salida, 'catalogo/index.html', miles(pagina_catalogo(por_sis, sysn, base, n_trad, n_juegos, filas)))
    escribir(salida, 'novedades/index.html', miles(pagina_novedades(dias, filas, sysn, base)))
    escribir(salida, 'novedades.xml', feed_novedades(dias_publicas, sysn, base))
    indice = []
    for g in juegos:
        escribir(salida, g['ruta'] + 'index.html', pagina_juego(g, sysn, libretro, base, ctx))
        busca = ' '.join(dict.fromkeys(sum((tokens(f['juego']) + tokens(f['titulo_original']) for f in g['filas']), [])
                                       + [sin_acentos(f['grupo']).lower() for f in g['filas'] if f['grupo']]))
        indice.append([g['titulo'], g['s'], g['ruta'], len(g['filas']), int(any(directa(f) for f in g['filas'])), busca, portada_spec(g), banderas(g)])
    indice.sort(key=lambda j: sin_acentos(j[0]).lower())
    escribir(salida, 'catalogo/juegos.json', json.dumps(indice, ensure_ascii=False, separators=(',', ':')))
    escribir(salida, 'catalogo/parches.json', parches_json(filas))
    escribir(salida, 'parchear/index.html', pagina_parchear(base, sysn))
    escribir(salida, 'mis-listas/index.html', pagina_listas(base))
    for s, gs in por_sis.items():
        escribir(salida, f'sistema/{s}/index.html', miles(pagina_sistema(s, gs, sysn, base)))
    escribir(salida, '404.html', pagina_404(base))

    # La fecha de cada página es la de su última traducción (los buscadores ignoran un lastmod que siempre es hoy);
    # la portada y el catálogo cambian cada vez que se genera la web.
    hoy = date.today().isoformat()
    rutas = ([('', hoy), ('catalogo/', hoy), ('novedades/', hoy), ('parchear/', hoy)]
             + [(f'sistema/{s}/', max((g['fecha'] for g in gs), default='')) for s, gs in por_sis.items()]
             + [(g['ruta'], g['fecha']) for g in juegos])
    escribir(salida, 'sitemap.xml', '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
             + ''.join(f'<url><loc>{e(base + r)}</loc>' + (f'<lastmod>{d}</lastmod>' if d else '') + '</url>\n' for r, d in rutas) + '</urlset>\n')
    escribir(salida, 'robots.txt', f'User-agent: *\nAllow: /\nSitemap: {base}sitemap.xml\n')
    print(f'Web en {salida}: {n_juegos} juegos, {len(por_sis)} sistemas, {len(rutas)} páginas; novedades: {len(altas)} altas en {len(dias)} días. Dirección: {base}')


if __name__ == '__main__':
    main()
