# Genera la web de la Biblioteca de traducciones (GitHub Pages) a partir del catálogo: la portada, el
# buscador, una página por juego y otra por sistema. Solo usa la biblioteca estándar de Python.
# Uso: python tools/web.py [carpeta de salida, _web por defecto]
# El dominio va en web/CNAME; si no está, la web queda en la dirección de GitHub Pages. Los ficheros de
# verificación de Google Search Console (google….html) y Bing (BingSiteAuth.xml) se dejan en web/ y van a la raíz.
import collections, csv, html, io, json, os, random, re, shutil, subprocess, sys, unicodedata
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
# Las listas de los bloqueadores de anuncios (EasyPrivacy) cortan gc.zgo.at y goatcounter.com como terceros. Por eso
# el script va en la propia web (web/estadisticas.js, el count.js de GoatCounter, licencia ISC) y las visitas se
# mandan a un subdominio propio, un CNAME a <código>.goatcounter.com puesto también en los ajustes de GoatCounter.
# Vacío, se usan gc.zgo.at y <código>.goatcounter.com.
GOATCOUNTER_DOMINIO = 'stats.bibliotecadetraducciones.com'
INCIDENCIAS = REPO + '/issues'
LIBRETRO = {}  # nombres de los sistemas en thumbnails.libretro.com (de centro_datos.json); lo rellena main()
PC = {'dos', 'windows', 'scummvm', 'easyrpg', 'pc'}
# Consolas de hoy: sus traducciones son mods que se instalan a mano (como MODERNAS en ui/app.js).
MODERNAS = {'switch', 'ps3', 'ps4', 'ps5', 'wiiu', 'xbox360', 'xboxone'}
e = html.escape

# ---------------------------------------------------------------- SEO
SITIO = 'Biblioteca de traducciones'
# Los buscadores cortan el título hacia los 60 caracteres y la descripción hacia los 155. El nombre de la web ya lo
# enseña Google encima de cada resultado (og:site_name y el WebSite de la portada), así que los títulos no lo repiten.
MAX_TITULO = 65
MAX_DESC = 158
# El nombre corto con el que se busca cada sistema, para el título de las fichas («Chrono Trigger en español (SNES)»).
# Los que no están aquí van con su nombre de siempre (sistemas_app()).
SIS_CORTO = {'snes': 'SNES', 'gba': 'GBA', 'gbc': 'GBC', 'n64': 'N64', 'nds': 'NDS', '3ds': '3DS', 'psx': 'PS1',
             'ps2': 'PS2', 'ps3': 'PS3', 'ps4': 'PS4', 'ps5': 'PS5', 'psvita': 'PS Vita', 'switch': 'Switch',
             'windows': 'PC', 'fds': 'Famicom Disk', 'ngpc': 'NGPC', 'ngp': 'NGP'}


def sis_corto(s, sysn):
    return SIS_CORTO.get(s, sysn.get(s, s))


def recorta(t, maximo=MAX_DESC):
    """Corta un texto en el último espacio antes de `maximo` y le pone puntos suspensivos."""
    if len(t) <= maximo:
        return t
    return t[:maximo - 1].rsplit(' ', 1)[0].rstrip(',;:· ') + '…'


def el_mas_largo_que_quepa(opciones, maximo):
    """La primera de las opciones (de la más completa a la más corta) que cabe; si ninguna, la última recortada."""
    for t in opciones:
        if len(t) <= maximo:
            return t
    return recorta(opciones[-1], maximo)

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
    # El tirón de cada juego (0-100), de herramientas\mantenimiento\tiron.py: elige las portadas del mosaico de la portada.
    pop, ruta_pop = {}, os.path.join(WEB, 'popularidad.tsv')
    if os.path.isfile(ruta_pop):
        for l in open(ruta_pop, encoding='utf-8'):
            if not l.startswith('#') and '\t' in l:
                k, p = l.rstrip('\n').split('\t')[:2]
                pop[k] = int(p)
    # Los datos de cada juego (año, desarrolladora, géneros, capturas, vídeo…), de herramientas\mantenimiento\datos_launchbox.py.
    ruta_dj = os.path.join(WEB, 'datos_juegos.json')
    dj = json.load(open(ruta_dj, encoding='utf-8')) if os.path.isfile(ruta_dj) else {}
    for f in filas:
        clave = f"{f['sistema']}|{f['juego']}|{f['grupo']}"
        f['x'] = centro['filas'].get(clave, {})
        f['pop'] = pop.get(clave, 0)
        f['dj'] = dj.get(clave, {})
    return filas, centro['sistemas']


def fijados():
    """Los juegos que van siempre en el mosaico de la portada (web/destacados.txt: sistema|título, uno por línea)."""
    ruta = os.path.join(WEB, 'destacados.txt')
    if not os.path.isfile(ruta):
        return []
    return [tuple(l.strip().split('|', 1)) for l in open(ruta, encoding='utf-8') if l.strip() and not l.startswith('#') and '|' in l]


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
             'fecha': max((fecha_iso(f) for f in fs), default=''),
             'dj': max((f['dj'] for f in [cover] + fs), key=len, default={})}
        for f in fs:
            f['gr'] = g
        out.append(g)
    return out


def fecha_iso(f):
    """La fecha de la traducción como AAAA-MM-DD, o '' si no está entera."""
    d = f['fecha'].strip()
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', d):
        return ''
    try:
        # Search Console marca el sitemap con error si una fecha es imposible, y «0018-11-08» no es una fecha.
        return d if date.fromisoformat(d).year >= 1990 else ''
    except ValueError:
        return ''


def anio(f):
    m = re.match(r'\d{4}', f['fecha'].strip())
    return m.group(0) if m else ''


def es_ia(f):
    return f['notas'].startswith('[IA]')


# La variante del español, cuando las notas o el grupo la dicen. Casi nunca la dicen: entonces no se afirma nada.
RE_LATINO = re.compile(r'latino|latinoam[eé]ric|español de am[eé]rica|español americano|mexican|argentin|chilen|colombian|venezolan|peruan|uruguay')
RE_NEUTRO = re.compile(r'\bneutr[oa]\b')
RE_ESPANA = re.compile(r'español de españa|castellano|peninsular|\bespaña\b')
VARIANTES = {'latino': 'latino', 'neutro': 'neutro', 'espana': 'de España'}


def variante(f):
    t = (f['notas'] + ' ' + f['grupo']).lower()
    if RE_LATINO.search(t):
        return 'latino'
    if RE_NEUTRO.search(t):
        return 'neutro'
    if RE_ESPANA.search(t):
        return 'espana'
    return ''


# El doblaje: las notas del catálogo lo dicen con una frase fija («Doblaje al castellano.», «Traducción y doblaje al
# español latino.», «Doblaje parcial al…»; ver catalogo\LEEME_catalogo.txt). 'doblaje' si solo pone voces (el juego ya
# tenía los textos en español), 'ambos' si traduce y dobla.
RE_DOBLAJE = re.compile(r'(Traducción y doblaje|Doblaje)( parcial)? al (castellano|español)')


def doblaje(f):
    m = RE_DOBLAJE.search(f['notas'])
    return '' if not m else ('doblaje' if m.group(1) == 'Doblaje' else 'ambos')


def tiene_logros(f):
    """La versión de esta traducción que guarda RetroAchievements en GitHub (RAPatches): con ella, el juego traducido tiene logros."""
    return 'RAPatches' in json.dumps(f['x'].get('d', '')) + f['url'] + f['fuente']


# ---------------------------------------------------------------- grupos de traducción
GRUPO_ALIAS = {'Wave': 'Traducciones Wave', 'Charnego': 'CharnegoTranslations'}
MIN_GRUPO = 3  # traducciones que necesita un grupo para tener página propia


def partes_grupo(grupo):
    """Los grupos o personas de una ficha: «pitito / Wesker» son dos."""
    return [GRUPO_ALIAS.get(p.strip(), p.strip()) for p in re.split(r'\s*/\s*|\s*;\s*', grupo) if p.strip()]


RUTAS_GRUPO = {}  # nombre -> ruta, sin repetir (la rellena main)


def ruta_grupo(nombre):
    return RUTAS_GRUPO.get(nombre) or f'grupo/{slug(nombre)}/'


# ---------------------------------------------------------------- el juego: datos de LaunchBox
GENEROS = {'Platform': 'plataformas', 'Action': 'acción', 'Shooter': 'disparos', 'Role-Playing': 'rol', 'Adventure': 'aventura',
           'Puzzle': 'puzles', "Beat 'em Up": "beat 'em up", 'Fighting': 'lucha', 'Sports': 'deportes', 'Strategy': 'estrategia',
           'Horror': 'terror', 'Racing': 'carreras', 'Visual Novel': 'novela visual', 'Life Simulation': 'simulación social',
           'Construction and Management Simulation': 'gestión', 'Board Game': 'juego de mesa', 'Stealth': 'sigilo',
           'Flight Simulator': 'simulación de vuelo', 'Sandbox': 'mundo abierto', 'Compilation': 'recopilatorio', 'Pinball': 'pinball',
           'Education': 'educativo', 'Vehicle Simulation': 'simulación de vehículos', 'Party': 'party', 'MMO': 'MMO', 'Quiz': 'preguntas',
           'Music': 'musical', 'Casino': 'casino'}
IMG_LB = 'https://images.launchbox-app.com/'


def enumera(lista):
    lista = [x for x in lista if x]
    return lista[0] if len(lista) == 1 else ', '.join(lista[:-1]) + ' y ' + lista[-1] if lista else ''


def descripcion_juego(gr, nombre_sis):
    """Un párrafo propio con los datos de LaunchBox (no se copia ninguna descripción): género, quién lo hizo, cuándo y para cuántos."""
    d, titulo = gr['dj'], gr['titulo']
    if not d:
        return ''
    gen = [GENEROS.get(g, '') for g in d.get('g', [])]
    gen = [g for g in dict.fromkeys(gen) if g]
    if 'novela visual' in gen:
        que = 'una novela visual'
    elif 'recopilatorio' in gen:
        que = 'un recopilatorio'
    elif 'juego de mesa' in gen:
        que = 'un juego de mesa'
    else:
        que = 'un juego' + (f' de {enumera(gen[:3])}' if gen else '')
    frase = f'{titulo} es {que} para {nombre_sis}'
    dev, edi, a = d.get('d', ''), d.get('e', ''), d.get('a', '')
    if dev and (not edi or edi == dev or edi.startswith(dev) or dev.startswith(edi)):
        frase += f', de {dev}' + (f', que lo publicó en {a}' if a else '')
    elif dev:
        frase += f', desarrollado por {dev}' + (f' y publicado por {edi}' if edi else '') + (f' en {a}' if a else '')
    elif edi:
        frase += f', publicado por {edi}' + (f' en {a}' if a else '')
    elif a:
        frase += f', de {a}'
    frase += '.'
    j = d.get('j', '')
    if j.isdigit() and int(j) > 0:
        frase += ' Es para un jugador.' if j == '1' else f' Se juega hasta con {j} jugadores' + (', también en cooperativo.' if d.get('c') else '.')
    return frase


def seccion_juego(gr, nombre_sis):
    """«El juego»: la descripción, sus datos, capturas del juego original y el vídeo (que no se carga hasta que lo pulsas)."""
    d = gr['dj']
    if not d:
        return ''
    titulo = gr['titulo']
    datos = [(k, v) for k, v in (('Año', d.get('a')), ('Desarrolladora', d.get('d')), ('Editora', d.get('e') if d.get('e') != d.get('d') else ''),
                                 ('Género', enumera([GENEROS.get(g, g) for g in d.get('g', [])][:3]).capitalize()),
                                 ('Jugadores', d.get('j') + (' (cooperativo)' if d.get('c') else '') if d.get('j') else '')) if v]
    dl = ''.join(f'<dt>{k}</dt><dd>{e(v)}</dd>' for k, v in datos)
    imgs = ([d['t']] if d.get('t') else []) + d.get('s', [])
    capturas = ''.join(f'<a href="{e(IMG_LB + i)}" target="_blank" rel="noopener"><img loading="lazy" src="{e(IMG_LB + i)}" alt="Captura de {e(titulo)}" '
                       f'referrerpolicy="no-referrer" onerror="this.parentNode.remove()"></a>' for i in imgs[:4])
    v = d.get('v', '')
    video = (f'<button type="button" class="video" data-v="{e(v)}" title="Vídeo de {e(titulo)}" onclick="yt(this)">'
             f'<img loading="lazy" src="https://i.ytimg.com/vi/{e(v)}/hqdefault.jpg" alt="" referrerpolicy="no-referrer"><span>Ver un vídeo del juego</span></button>'
             if re.fullmatch(r'[A-Za-z0-9_-]{11}', v) else '')
    w = web_segura(d.get('w', ''))
    wiki = (f'<a href="{e(w)}" target="_blank" rel="noopener">Más sobre el juego en la Wikipedia{" (en inglés)" if "en.wikipedia" in w else ""}</a>' if w else '')
    return f'''<section class="bloque el-juego">
    <h2>El juego</h2>
    <p class="texto">{e(descripcion_juego(gr, nombre_sis))}</p>
    {f'<dl class="datos">{dl}</dl>' if dl else ''}
    {f'<div class="capturas">{capturas}</div><p class="suave">Capturas del juego original, sin traducir (de LaunchBox).</p>' if capturas else ''}
    {video}
    {f'<p class="suave">{wiki}{" · " if wiki and video else ""}{"El vídeo es de YouTube y no se carga hasta que lo pulsas." if video else ""}</p>' if wiki or video else ''}
  </section>'''


def adulto(f):
    """Traducción de un juego para adultos (+18): la marca a18 de centro_datos (de cache\adultos.tsv)."""
    return bool(f['x'].get('a18'))


def juego_adulto(g):
    """Un juego se oculta cuando todas sus traducciones son de adultos."""
    return all(adulto(f) for f in g['filas'])


def banderas(g):
    """Lo que filtra el buscador (catalogo.js): 1 alguna completa, 2 alguna sin IA, 4 se vende en Steam, 8 para adultos,
    16 alguna en español latino o neutro, 32 alguna con doblaje, 64 alguna con logros de RetroAchievements."""
    return ((1 if any(f['estado'].strip().lower().startswith('complet') for f in g['filas']) else 0)
            | (2 if any(not es_ia(f) for f in g['filas']) else 0)
            | (4 if any(f['x'].get('steam') for f in g['filas']) else 0)
            | (8 if juego_adulto(g) else 0)
            | (16 if any(variante(f) in ('latino', 'neutro') for f in g['filas']) else 0)
            | (32 if any(doblaje(f) for f in g['filas']) else 0)
            | (64 if any(tiene_logros(f) for f in g['filas']) else 0))


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
    tipo = doblaje(f)
    datos = []
    if anio(f):
        datos.append(f'es de {anio(f)}')
    est, ver = ESTADOS.get(f['estado'].strip().lower()), f['version'].strip()
    if est and tipo == 'doblaje':  # «el doblaje está completo»
        est = est.replace('completa', 'completo').replace('publicada', 'publicado').replace('abandonada', 'abandonado')
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
    sujeto = {'doblaje': 'El doblaje', 'ambos': 'La traducción con doblaje'}.get(tipo, 'La traducción')
    frase = f'{sujeto} de {quien}' + ((', hecho con voces generadas por IA,' if tipo == 'doblaje' else ', hecha con IA o con traducción automática,')
                                      if es_ia(f) else '')
    if datos:
        frase += ' ' + (datos[0] if len(datos) == 1 else ', '.join(datos[:-1]) + ' y ' + datos[-1])
    else:
        frase += ' no tiene más datos en el catálogo'
    if directa(f):
        frase += '; la Biblioteca de traducciones ' + ('lo descarga y lo aplica' if tipo == 'doblaje' else 'la descarga y la aplica') + ' con un clic'
    return frase + '.'


def resumen_juego(gr, nombre_sis):
    """Dos párrafos propios de cada juego, con las palabras con las que la gente busca."""
    titulo, fs = gr['titulo'], gr['filas']
    orig, n = titulo_original(gr), len(fs)
    vars_ = sorted({VARIANTES[v] for v in (variante(f) for f in fs) if v})
    esp = 'al español' + (f' ({" y ".join(vars_)})' if vars_ else '')
    tipos = [doblaje(f) for f in fs]
    k = sum(1 for t in tipos if t)
    if n == 1:
        cosa, hecha, aplicar = {'doblaje': ('su doblaje', 'hecho', 'aplicarlo'),
                                'ambos': ('su traducción con doblaje', 'hecha', 'aplicarla')}.get(tipos[0], ('su traducción', 'hecha', 'aplicarla'))
        reune = f'Esta ficha reúne {cosa} {esp}, {hecha} por aficionados, con sus datos y cómo {aplicar} a tu copia.'
    elif all(t == 'doblaje' for t in tipos):
        reune = f'Esta ficha reúne sus {n} doblajes {esp}, hechos por aficionados, con sus datos y cómo aplicarlos a tu copia.'
    else:
        reune = (f'Esta ficha reúne sus {n} traducciones {esp}, hechas por aficionados' + (f' ({"una" if k == 1 else k} con doblaje)' if k else '')
                 + ', con sus datos y cómo aplicarlas a tu copia.')
    # Con datos de LaunchBox, «El juego» ya lo presenta; aquí solo hace falta el título original, si lo hay.
    intro = ((f'Su título original es {orig}. ' if orig else '') + reune if gr['dj'] else
             f'{titulo} es un juego de {nombre_sis}' + (f' (título original: {orig})' if orig else '') + '. ' + reune)
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
# Antes de esta fecha el catálogo se estaba reordenando en el repositorio (duplicados, títulos corregidos) y cada
# retoque parecería una ficha nueva: lo anterior cuenta como fondo, no como novedad.
NOVEDADES_DESDE = '2026-11-03'
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
    """La clave con la que se sigue una fila por el historial: sistema, título y grupo sin acentos ni signos, para que
    corregir unas mayúsculas o una coma no la convierta en una ficha nueva."""
    return f['sistema'] + '|' + re.sub(r'[^a-z0-9]+', ' ', sin_acentos(f['juego']).lower()).strip() + '|' + re.sub(r'[^a-z0-9]+', ' ', sin_acentos(f['grupo']).lower()).strip()


FAMILIA = {**dict.fromkeys(['nes', 'fds', 'snes', 'satellaview', 'gb', 'gbc', 'gba', 'n64', 'virtualboy'], 'nintendo clásica'),
           **dict.fromkeys(['gamecube', 'wii', 'wiiu', 'nds', '3ds', 'switch'], 'nintendo moderna'),
           **dict.fromkeys(['sg1000', 'mastersystem', 'gamegear', 'megadrive', 'megacd', 'sega32x', 'saturn', 'dreamcast'], 'sega'),
           **dict.fromkeys(['psx', 'ps2', 'ps3', 'ps4', 'ps5', 'psp', 'psvita'], 'sony'),
           **dict.fromkeys(['pc', 'dos', 'windows', 'scummvm', 'easyrpg', 'msx', 'msx2', 'pc98', 'amiga', 'c64', 'zxspectrum', 'amstradcpc'], 'ordenador')}


def destacados(filas, dias, pins, k=9):
    """Las portadas del mosaico de la portada: primero los juegos fijados en web/destacados.txt, luego las novedades de
    los últimos días con tirón (web/popularidad.tsv) y, para completar, los juegos de más tirón de todo el catálogo,
    que rotan cada día. Sin IA ni juegos para adultos, con portada, y como mucho dos juegos por sistema y tres por
    familia, para que no salgan nueve de Super Nintendo. Devuelve [(juego, direcciones de la portada)]."""
    elegidos, titulos, por_sis, por_fam = [], set(), collections.Counter(), collections.Counter()

    def cabe(g):
        t = sin_acentos(g['titulo']).lower()
        if t in titulos or juego_adulto(g) or all(es_ia(f) for f in g['filas']):
            return False
        fam = FAMILIA.get(g['s'], 'otros')
        if por_sis[g['s']] >= 2 or por_fam[fam] >= 3:
            return False
        urls = portadas(g, LIBRETRO, '')
        if not urls:
            return False
        elegidos.append((g, urls))
        titulos.add(t)
        por_sis[g['s']] += 1
        por_fam[fam] += 1
        return True

    juegos = list({id(f['gr']): f['gr'] for f in filas}.values())
    for s, j in pins:
        g = next((g for g in juegos if g['s'] == s and sin_acentos(g['titulo']).lower() == sin_acentos(j).lower()), None)
        if g:
            cabe(g)
    tope = len(elegidos) + min(4, k - len(elegidos))
    for f in sorted((f for _, fs in dias for f in fs if f['pop'] >= 60 and not es_ia(f)), key=lambda f: (-f['pop'], f['alta'])):
        if len(elegidos) >= tope:
            break
        cabe(f['gr'])
    top = sorted((g for g in juegos if any(f['pop'] for f in g['filas'])), key=lambda g: -max(f['pop'] for f in g['filas']))[:60]
    random.Random(date.today().isoformat()).shuffle(top)
    for g in top:
        if len(elegidos) >= k:
            break
        cabe(g)
    return elegidos[:k]


def fechas_alta():
    """Cuándo entró cada traducción en el catálogo, según el historial de git del CSV: clave de la fila -> AAAA-MM-DD.
    Se miran los commits de los últimos DIAS_NOVEDADES días; las filas que ya estaban en el más antiguo de ellos no
    llevan fecha, porque no se sabe cuándo entraron. Si en un commit desaparece una fila y aparece la del mismo juego con
    los mismos autores y alguno más (se acredita a quien no constaba, o se suma quien la retocó), es la misma ficha:
    conserva su fecha y no sale como novedad. Sin git o sin historial (un clon superficial), no hay fechas."""
    ruta = 'src-tauri/recursos/catalogo/traducciones_fan.csv'

    def git(*args):
        return subprocess.run(['git', '-C', RAIZ, *args], capture_output=True, check=True).stdout

    try:
        commits = [l.split() for l in git('log', '--format=%H %cs', f'--since={DIAS_NOVEDADES} days ago', '-n', '500', '--', ruta)
                   .decode().splitlines() if l.strip()]
        if len(commits) < 2:
            return {}
        altas, conocidas, previas = {}, set(), set()
        for i, (sha, fecha) in enumerate(reversed(commits)):  # del más antiguo al más nuevo
            claves = {clave_fila(f) for f in csv.DictReader(io.StringIO(git('show', f'{sha}:{ruta}').decode('utf-8-sig')))}
            base = i == 0 or fecha < NOVEDADES_DESDE
            idas = previas - claves  # las filas que desaparecen en este commit: borradas o con otro grupo
            for c in claves - conocidas:
                if not base:
                    sis, jue, gru = c.split('|', 2)
                    autores = set(gru.split())
                    antes = next((k for k in idas if k.startswith(f'{sis}|{jue}|') and set(k.split('|', 2)[2].split()) <= autores), None)
                    if antes is None:
                        altas[c] = fecha
                    elif antes in altas:
                        altas[c] = altas[antes]
                conocidas.add(c)
            previas = claves
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
def pagina(raiz, titulo, descripcion, ruta, cuerpo, base, imagen='img/vista-previa.png', tipo='website', ld=None, indexar=True, previa=''):
    """`indexar=False` pide a los buscadores que no guarden la página (Mis listas y la 404, sin contenido propio);
    `previa` es el origen (https://host) de la imagen principal, para abrir la conexión antes de que el navegador la pida."""
    canon = base + ruta
    og_imagen = imagen if imagen.startswith('http') else base + imagen
    extra = ('' if indexar else '<meta name="robots" content="noindex, follow">\n') + (
        f'<link rel="preconnect" href="{e(previa)}">\n' if previa else '')
    cuenta = ''
    if GOATCOUNTER:
        destino = f'https://{GOATCOUNTER_DOMINIO or GOATCOUNTER + ".goatcounter.com"}/count'
        script = f'{raiz}estadisticas.js' if os.path.isfile(os.path.join(WEB, 'estadisticas.js')) else '//gc.zgo.at/count.js'
        cuenta = f'<script data-goatcounter="{destino}" async src="{script}"></script>\n'
    nota_cuenta = ' Estadísticas anónimas y sin cookies con <a href="https://www.goatcounter.com/">GoatCounter</a>.' if GOATCOUNTER else ''
    return f'''<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="strict-origin">
<title>{e(titulo)}</title>
<meta name="description" content="{e(descripcion)}">
{extra}<link rel="canonical" href="{e(canon)}">
<link rel="icon" href="{raiz}icono.svg" type="image/svg+xml">
<link rel="stylesheet" href="{raiz}estilo.css">
<link rel="alternate" type="application/atom+xml" title="Novedades del catálogo" href="{raiz}novedades.xml">
<meta property="og:site_name" content="{SITIO}">
<meta property="og:type" content="{tipo}">
<meta property="og:title" content="{e(titulo)}">
<meta property="og:description" content="{e(descripcion)}">
<meta property="og:url" content="{e(canon)}">
<meta property="og:image" content="{e(og_imagen)}">
<meta property="og:locale" content="es_ES">
<meta name="twitter:card" content="summary_large_image">
{jsonld(ld) if ld else ''}
<script>try{{if(localStorage.getItem('adultos')==='1')document.documentElement.classList.add('adultos')}}catch(e){{}}function sig(i){{var r=(i.dataset.alt||'').split(' ').filter(Boolean);if(!r.length){{i.parentNode.classList.add('sin');i.remove();return}}i.src=r.shift();i.dataset.alt=r.join(' ')}}function yt(b){{if(!/^[A-Za-z0-9_-]{{11}}$/.test(b.dataset.v))return;var f=document.createElement('iframe');f.src='https://www.youtube-nocookie.com/embed/'+b.dataset.v+'?autoplay=1';f.allow='autoplay; encrypted-media; picture-in-picture; fullscreen';f.allowFullscreen=true;f.title=b.title;f.className='video';b.replaceWith(f)}}</script>
</head>
<body>
<div class="franja" aria-hidden="true"></div>
<header class="barra">
  <a class="marca" href="{raiz or './'}"><img src="{raiz}icono.svg" alt="" width="32" height="32"><span>Biblioteca de traducciones</span></a>
  <nav aria-label="Principal">
    <a href="{raiz}catalogo/">Catálogo</a>
    <a href="{raiz}novedades/">Novedades</a>
    <a href="{raiz}selecciones/">Selecciones</a>
    <a href="{raiz}doblajes/">Doblajes</a>
    <a href="{raiz}mis-listas/" id="nav-listas">Mis listas</a>
    <a class="btn primario" href="{INSTALADOR}" data-goatcounter-click="descarga-cabecera">Descargar</a>
  </nav>
</header>
{cuerpo}
<footer class="pie">
  <div class="pie-in">
    <p><b>Biblioteca de traducciones</b> es gratuita y de código libre (<a href="{REPO}/blob/main/LICENSE">GPL-3.0</a>). Las traducciones son de sus autores: este programa solo las encuentra y las aplica, y nunca descarga juegos.</p>
    <p><a href="{raiz}grupos/">Grupos de traducción</a> · <a href="{raiz}en-camino/">Traducciones en camino</a> · <a href="{raiz}selecciones/">Selecciones</a> · <a href="{raiz}parchear/">Parchear en el navegador</a></p>
    <p><a href="{REPO}" data-goatcounter-click="estrella-github">Código en GitHub: si te es útil, dale una estrella ★</a> · <a href="{INCIDENCIAS}">Avisar de un error o de una traducción que falta</a></p>
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
    if variante(f):
        out.append(f'<span class="chip var" title="La variante del español, según sus autores">Español {VARIANTES[variante(f)]}</span>')
    if doblaje(f):
        parcial = bool(RE_DOBLAJE.search(f['notas']).group(2))
        out.append(f'<span class="chip dob" title="Lleva voces en español{" en una parte del juego" if parcial else ""}">'
                   f'{"Doblaje" if doblaje(f) == "doblaje" else "Con doblaje"}{" parcial" if parcial else ""}</span>')
    if tiene_logros(f):
        out.append('<span class="chip ra" title="Con la versión de este parche que guarda RetroAchievements, el juego traducido tiene logros">Logros</span>')
    return ''.join(out)


def formato(f):
    fmt = f['formato_parche'].strip()
    return fmt.upper() if fmt.lower() in ('ips', 'bps', 'ups', 'ppf', 'aps', 'xdelta') else fmt.capitalize()


# Dónde se aplica cada traducción de consola, para elegir el botón principal de su ficha.
NAVEGADOR = {'ips', 'bps', 'ups', 'ppf', 'aps'}  # los que aplica web/motor.js
# Sistemas de disco o de ROMs grandes: sin formato apuntado, lo normal es un parche xdelta, que el navegador no aplica
# (en el catálogo de 2026-11, xdelta es el formato conocido más común en todos ellos).
DISCO = {'psx', 'ps2', 'psp', 'psvita', 'dreamcast', 'saturn', 'megacd', 'pcenginecd', 'neogeocd', '3do', 'pc98',
         'gamecube', 'wii', 'xbox', 'nds', '3ds'}


def donde_aplicar(f):
    """'navegador' (IPS, BPS, UPS, PPF o APS: Parchear en el navegador), 'programa' (xdelta: el programa para Windows)
    o 'manual' (archivos, mods, instaladores: se instalan como diga su autor). Sin formato apuntado, depende del
    sistema: en los de cartucho casi todo es IPS; en los de disco, xdelta."""
    fmts = set(re.split(r'[/,+\s]+', f['formato_parche'].strip().lower())) - {''}
    if fmts & NAVEGADOR:
        return 'navegador'
    if 'xdelta' in fmts:
        return 'programa'
    if fmts - {'parche'}:
        return 'manual'
    return 'programa' if f['sistema'] in DISCO else 'navegador'


GRUPOS_CON_PAGINA = set()  # los rellena main: los grupos con MIN_GRUPO traducciones o más


def nombre_grupo_html(grupo, raiz):
    """El grupo de una ficha con enlace a la página de cada parte que la tenga."""
    if not grupo.strip():
        return 'Autor desconocido'
    partes = partes_grupo(grupo)
    if not any(p in GRUPOS_CON_PAGINA for p in partes):
        return e(grupo)
    return ' / '.join(f'<a href="{raiz}{ruta_grupo(p)}">{e(p)}</a>' if p in GRUPOS_CON_PAGINA else e(p) for p in partes)


def tarjeta_trad(f, raiz='../../../', ruta=''):
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
    cosa = 'del doblaje' if doblaje(f) == 'doblaje' else 'de la traducción'
    # GoatCounter cuenta los clics como el evento «trad-<sistema>/<juego>»: es lo más parecido a las descargas de cada ficha.
    evento = (f' data-goatcounter-click="trad-{e(ruta.removeprefix("juego/").strip("/"))}" '
              f'data-goatcounter-title="{e(f["juego"])} ({e(f["grupo"][:60])})"') if ruta and GOATCOUNTER else ''
    enlace = (f'<a class="btn" href="{e(f["url"])}" target="_blank" rel="noopener"{evento}>Página {cosa}</a>'
              if web_segura(f['url']) else f'<p class="suave">No se conoce la página {"de este doblaje" if doblaje(f) == "doblaje" else "de esta traducción"}.</p>')
    return f'''<article class="trad">
  <div class="trad-h"><h3>{nombre_grupo_html(f['grupo'], raiz)}</h3><div class="chips">{chips_trad(f)}</div></div>
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
    # La portada es lo más grande que se ve al abrir la ficha (el LCP): se pide la primera y con la conexión ya abierta.
    img = (f'<img src="{e(urls[0])}" data-alt="{e(" ".join(urls[1:]))}" alt="Portada de {e(titulo)}" '
           f'fetchpriority="high" referrerpolicy="no-referrer" onerror="sig(this)">') if urls else ''
    previa = re.match(r'https://[^/]+', urls[0]).group(0) if urls and urls[0].startswith('https://') else ''
    # Para las redes y los buscadores, la portada con dirección completa.
    absolutas = portadas(gr, libretro, base)
    imagen = absolutas[0] if absolutas else 'img/vista-previa.png'
    quien = ', '.join(grupos[:3]) + (' y otros' if len(grupos) > 3 else '')
    orig = titulo_original(gr)
    # Los que están en Steam (tienen AppID): enlace a su página de la tienda, sin afiliado.
    en_steam = next((f for f in gr['filas'] if f['x'].get('steam')), None)
    compra = ''
    if en_steam:
        compra = f"""<div class="compra">
        <a class="btn tienda" href="https://store.steampowered.com/app/{e(str(en_steam['x']['steam']))}/" target="_blank" rel="noopener">¿No lo tienes? Página del juego en Steam</a>
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
    # Traducción, doblaje o las dos cosas: cambia el título, la entradilla y la descripción.
    tipos = [doblaje(f) for f in gr['filas']]
    k_dob = sum(1 for t in tipos if t)
    solo_dob = all(t == 'doblaje' for t in tipos)
    que = 'doblaje' if solo_dob else 'traducción y doblaje' if k_dob else 'traducción'
    if n == 1:
        cuantas = {'doblaje': 'Doblaje', 'ambos': 'Traducción y doblaje'}.get(tipos[0], 'Traducción')
        hay = {'doblaje': 'Hay un doblaje al español hecho por aficionados',
               'ambos': 'Hay una traducción con doblaje al español hecha por aficionados'}.get(tipos[0], 'Hay una traducción al español hecha por aficionados')
    elif solo_dob:
        cuantas, hay = f'{n} doblajes', f'Hay {n} doblajes al español hechos por aficionados'
    else:
        cuantas = f'{n} traducciones' + (' (con doblaje)' if k_dob else '')
        hay = f'Hay {n} traducciones al español hechas por aficionados' + (f', {"una" if k_dob == 1 else k_dob} con doblaje' if k_dob else '')
    # Dónde se aplica (donde_aplicar): elige el botón principal, el final de la descripción y los pasos de «Cómo jugarlo».
    # Basta una traducción que valga en el navegador para ofrecerlo; si no, el programa (xdelta) o la página del autor.
    modos = {donde_aplicar(f) for f in gr['filas']}
    modo = ('pc' if s in PC | MODERNAS else 'navegador' if 'navegador' in modos
            else 'programa' if 'programa' in modos else 'manual')
    xdelta = any('xdelta' in f['formato_parche'].lower() for f in gr['filas'])
    # La descripción para los buscadores: de la más completa a la más corta, la primera que quepa en MAX_DESC.
    if modo in ('pc', 'manual'):
        cierre = f'. Descárga{"lo" if solo_dob else "la"} gratis desde la página de su autor.' if n == 1 else '. Con el enlace a la página de cada autor.'
    elif modo == 'programa':
        cierre = '. Se aplica gratis con el programa para Windows.'
    else:
        cierre = '. Parchea tu ROM gratis en el navegador o con el programa.'

    def desc_con(k, det):
        q = (enumera(grupos) if len(grupos) <= k else ', '.join(grupos[:k]) + ' y otros') if k else ''
        return f'{cuantas} al español de {titulo} para {nombre_sis}' + (f', de {q}' if q else '') + det + cierre
    desc = el_mas_largo_que_quepa([desc_con(3, detalle), desc_con(2, detalle), desc_con(1, detalle), desc_con(1, ''),
                                   desc_con(0, '')], MAX_DESC)
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
    elif modo == 'programa':
        como = f"""<ol class="pasos">
      <li><b>Con el programa para Windows:</b> dile en qué carpeta tienes tus juegos, busca «{e(titulo)}» y pulsa <b>Traducir</b>. Comprueba que tu copia es la versión que pide la traducción y aplica el parche xdelta; si el juego está en .zip, .7z, .rar o .chd, lo saca y monta el disco traducido en .chd. Tu original no se toca.</li>
      <li><b>Sin Windows:</b> baja el parche de la página de su autor y aplícalo con un parcheador de xdelta, como Delta Patcher, sobre la versión del juego que pide.</li>
    </ol>
    <p class="suave">El navegador todavía no aplica parches xdelta. Necesitas tu propia copia del juego: ni la web ni el programa descargan juegos, solo los parches de traducción.</p>"""
    elif modo == 'manual':
        como = """<ol class="pasos">
      <li>Abre la página de la traducción y comprueba qué versión del juego pide.</li>
      <li>Instálala como explique su autor: normalmente, copiando sus archivos sobre los del juego o con su instalador.</li>
    </ol>
    <p class="suave">No es un parche, así que ni el navegador ni el programa la aplican solos. Necesitas tu propia copia del juego: ni la web ni el programa descargan juegos.</p>"""
    else:
        como = f"""<ol class="pasos">
      <li><b>Sin instalar nada:</b> abre <a href="{raiz}parchear/">Parchear en el navegador</a> y elige tu ROM, o toda tu carpeta de juegos. La página reconoce el juego por su CRC32, baja el parche de su autor (o usa el que hayas descargado tú) y te da el juego traducido. Nada sale de tu equipo.</li>
      <li><b>Con el programa para Windows:</b> dile en qué carpeta tienes tus juegos, busca «{e(titulo)}» y pulsa <b>Traducir</b>. Comprueba que tu copia es la versión que pide la traducción y la aplica; tu original no se toca.</li>
    </ol>
    <p class="suave">Necesitas tu propia copia del juego: ni la web ni el programa descargan juegos, solo los parches de traducción.</p>"""
    # El botón principal de la ficha, con una línea que explica por qué cuando no es el navegador.
    descarga = f'href="{INSTALADOR}" data-goatcounter-click="descarga-juego"'
    if modo == 'navegador':
        botones = (f'<div class="acciones-j"><a class="btn primario grande" href="{raiz}parchear/">Traducir en el navegador</a>'
                   f'<a class="btn grande" {descarga}>Programa para Windows</a></div>')
    elif modo == 'programa':
        por_que = ('El parche es xdelta y el navegador todavía no puede aplicarlo; el programa para Windows sí.' if xdelta else
                   f'Las traducciones de {e(nombre_sis)} suelen ser parches xdelta, que el navegador todavía no aplica; el programa '
                   f'para Windows sí. Si el parche resulta ser IPS, BPS, UPS, PPF o APS, también vale '
                   f'<a href="{raiz}parchear/">Parchear en el navegador</a>.')
        botones = f'<a class="btn primario grande" {descarga}>Traducir con el programa para Windows</a><p class="suave aviso-j">{por_que}</p>'
    elif modo == 'manual':
        autor = next((f for f in gr['filas'] if web_segura(f['url'])), None)
        evento = f' data-goatcounter-click="trad-{e(gr["ruta"].removeprefix("juego/").strip("/"))}"' if GOATCOUNTER else ''
        botones = ((f'<a class="btn primario grande" href="{e(autor["url"])}" target="_blank" rel="noopener"{evento}>Ir a la traducción</a>'
                    if autor else '<a class="btn primario grande" href="#traducciones">Ver la traducción</a>')
                   + '<p class="suave aviso-j">No es un parche: se instala como explica su autor, copiando sus archivos o con su instalador.</p>')
    else:
        botones = f'<a class="btn primario grande" {descarga}>Descargar el programa para Windows</a>'
    cuerpo = f"""<main class="juego">
  <nav class="migas" aria-label="Estás en"><a href="{raiz}">Inicio</a> › <a href="{raiz}catalogo/">Catálogo</a> › <a href="{raiz}sistema/{e(s)}/">{e(nombre_sis)}</a></nav>
  <div class="juego-h">
    <div class="portada{'' if img else ' sin'}">{img}<span class="ph">{e(titulo)}</span></div>
    <div class="juego-t">
      <p class="eti">{e(nombre_sis)}{' <span class="chip aviso" title="Juego para adultos">+18</span>' if juego_adulto(gr) else ''}</p>
      <h1>{e(titulo)} <span>en español</span></h1>
      {f'<p class="orig">Título original: {e(orig)}</p>' if orig else ''}
      <p class="lede">{hay}{f": {e(quien)}" if quien else ''}.</p>
      {botones}
      {compra}
      <div class="juego-acciones" data-ruta="{e(gr["ruta"])}" data-sis="{e(s)}" data-sisn="{e(nombre_sis)}" data-titulo="{e(titulo)}" data-port="{e(" ".join(absolutas[:3]))}"><button type="button" class="fav" aria-pressed="false" aria-label="Añadir a Favoritos"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 20s-7.5-4.6-9.3-9.2C1.4 7.4 3.6 4 7 4c2 0 3.6 1.1 5 3 1.4-1.9 3-3 5-3 3.4 0 5.6 3.4 4.3 6.8C19.5 15.4 12 20 12 20z"/></svg></button><button type="button" class="btn" id="guardar-lista">Guardar en una lista</button></div>
    </div>
  </div>
  <section class="bloque" id="traducciones">
    <h2>{({'doblaje': 'El doblaje', 'ambos': 'La traducción y el doblaje'}.get(tipos[0], 'La traducción') if n == 1
          else 'Los doblajes' if solo_dob else 'Las traducciones')}</h2>
    <div class="trads">{''.join(tarjeta_trad(f, ruta=gr['ruta']) for f in gr['filas'])}</div>
  </section>
  {seccion_juego(gr, nombre_sis)}
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
    # El título, con lo que se busca: «<juego> en español», el sistema por su nombre corto y «parche de traducción».
    # Dos juegos distintos con el mismo nombre en el mismo sistema (Monolith de 2017 y de 2023) llevan el año.
    corto = sis_corto(s, sysn)
    if (titulo, s) in ctx.get('repetidos', ()) and re.fullmatch(r'\d{4}', gr['dj'].get('a', '')):
        corto += ', ' + gr['dj']['a']
    # Si no cabe, se quita la coletilla pero nunca el sistema: el mismo juego suele estar en varios.
    if que == 'doblaje':
        opciones = [f'{titulo} ({corto}): doblaje al español']
    else:
        opciones = [f'{titulo} en español ({corto}): ' + ('parche de traducción' if que == 'traducción' else 'traducción y doblaje'),
                    f'{titulo} en español ({corto})']
    titulo_pagina = next((t for t in opciones if len(t) <= MAX_TITULO), opciones[-1])
    canon = base + gr['ruta']
    juego = {'@type': 'VideoGame', 'name': titulo, 'gamePlatform': nombre_sis, 'url': canon}
    if orig:
        juego['alternateName'] = orig
    dj = gr['dj']
    if dj.get('g'):
        juego['genre'] = [GENEROS.get(g, g) for g in dj['g']]
    if dj.get('d'):
        juego['author'] = {'@type': 'Organization', 'name': dj['d']}
    if dj.get('e'):
        juego['publisher'] = {'@type': 'Organization', 'name': dj['e']}
    if re.fullmatch(r'\d{4}', dj.get('a', '')):
        juego['datePublished'] = dj['a']
    if imagen.startswith('http'):
        juego['image'] = imagen
    ld = {'@context': 'https://schema.org', '@graph': [
        migas_ld(base, [('Inicio', ''), ('Catálogo', 'catalogo/'), (nombre_sis, f'sistema/{s}/'), (titulo, gr['ruta'])]),
        {'@type': 'WebPage', '@id': canon, 'url': canon, 'name': titulo_pagina, 'description': desc, 'inLanguage': 'es', 'about': juego}]}
    return pagina(raiz, titulo_pagina, desc, gr['ruta'], cuerpo, base, imagen=imagen, tipo='article', ld=ld, previa=previa)


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
    corto = sis_corto(s, sysn)
    nombres = f'{nombre} ({corto})' if corto != nombre and s != 'windows' else nombre
    titulo = el_mas_largo_que_quepa([f'Juegos de {nombres} en español: {n:,} {"traducción" if n == 1 else "traducciones"}',
                                     f'Juegos de {nombre} en español: {n:,} {"traducción" if n == 1 else "traducciones"}',
                                     f'Juegos de {nombre} en español'], MAX_TITULO)
    return pagina(raiz, titulo, desc, f'sistema/{s}/', cuerpo, base, ld=ld)


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
    <button type="button" class="chip" data-f="latino" aria-pressed="false" title="Traducciones en español latino o neutro, según sus autores">Español latino</button>
    <button type="button" class="chip" data-f="doblaje" aria-pressed="false" title="Juegos con voces en español hechas por aficionados">Doblajes</button>
    <button type="button" class="chip" data-f="logros" aria-pressed="false" title="Traducciones con las que RetroAchievements tiene logros">Con logros</button>
    <label class="chip-l"><input type="checkbox" id="adultos"> Mostrar juegos para adultos (+18)</label>
  </div>
  <p class="suave" id="cuenta" aria-live="polite"></p>
  <ul class="tarjetas" id="res"></ul>
  <button class="btn" id="mas" hidden>Ver más</button>
  <p class="suave explora-l">También: <a href="../selecciones/">Selecciones</a> · <a href="../doblajes/">Doblajes</a> · <a href="../grupos/">Grupos de traducción</a> · <a href="../en-camino/">Traducciones en camino</a> · <a href="../novedades/">Novedades</a></p>
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
    ld = {'@context': 'https://schema.org', '@graph': [
        migas_ld(base, [('Inicio', ''), ('Catálogo', 'catalogo/')]),
        {'@type': 'CollectionPage', 'url': base + 'catalogo/', 'name': 'Catálogo de traducciones de juegos al español', 'inLanguage': 'es'}]}
    return pagina(raiz, 'Catálogo de traducciones de juegos al español', desc, 'catalogo/', cuerpo, base, ld=ld)


def pagina_inicio(por_sis, sysn, base, n_trad, n_juegos, version, dias, filas, pins, sels=(), n_grupos=0):
    """La portada: el buscador con un mosaico de portadas al lado, las últimas novedades del catálogo (o, sin historial
    de git, las últimas traducciones por su fecha), las cifras, los sistemas y, ya después, el programa para Windows."""
    raiz = ''
    mosaico = ''.join(f'<a href="{e(g["ruta"])}" title="{e(g["titulo"])}"><img src="{e(u[0])}" data-alt="{e(" ".join(u[1:]))}" alt="" '
                      f'referrerpolicy="no-referrer" onerror="sig(this)" onload="this.classList.add(\'ok\')"><span class="ph">{e(g["titulo"])}</span></a>'
                      for g, u in destacados(filas, dias, pins))
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
    # Los doblajes de más tirón, un juego por título (Resident Evil 3 tiene tres sistemas).
    dob, vistos = [], set()
    for f in sorted((f for f in filas if doblaje(f) and not juego_adulto(f['gr'])), key=lambda f: -f['pop']):
        t = sin_acentos(f['gr']['titulo']).lower()
        if t not in vistos and portadas(f['gr'], LIBRETRO, raiz):
            vistos.add(t)
            dob.append(tarjeta_juego(f['gr'], raiz, sysn, True, fila=f))
    n_dob = len({id(f['gr']) for f in filas if doblaje(f)})
    bloque_dob = f'''<section class="bloque novedades doblajes">
    <div class="cab"><h2>Doblajes al español</h2><p class="suave"><a href="doblajes/">Los {n_dob} doblajes</a></p></div>
    <p class="texto suave">Juegos con voces en español gracias a grupos de aficionados, de Resident Evil y Silent Hill a Lies of P.</p>
    <ul class="tarjetas">{''.join(dob[:6])}</ul>
  </section>''' if dob else ''
    # «Lo más visto» (si hay datos de visitas de GoatCounter): sus 12 primeros juegos, sin los de adultos, justo después
    # de las novedades. Por eso no va también entre las selecciones de «Explora».
    todos_vistos = next((s[3] for s in sels if s[0] == 'lo-mas-visto'), [])
    vistos = [g for g in todos_vistos if not juego_adulto(g)][:12]
    bloque_vistos = f'''<section class="bloque novedades vistos">
    <div class="cab"><h2>Lo más visto</h2><p class="suave"><a href="selecciones/lo-mas-visto/">Los {len(todos_vistos)} más vistos</a></p></div>
    <p class="texto suave">Los juegos cuyas fichas más se han consultado en la web en los últimos {MAS_VISTOS_DIAS} días.</p>
    <ul class="tarjetas">{''.join(tarjeta_juego(g, raiz, sysn, True) for g in vistos)}</ul>
  </section>''' if len(vistos) >= 6 else ''
    portada_sels = explora(sels)
    usadas = set()  # que una misma portada no salga en dos tarjetas
    bloque_explora = (f'''<section class="bloque explora">
    <div class="cab"><h2>Explora</h2><p class="suave"><a href="selecciones/">Todas las selecciones</a></p></div>
    <div class="sels">{''.join(tarjeta_seleccion(s, raiz, evitar=usadas) for s in portada_sels)}</div>
    <p class="suave">Y también: <a href="grupos/">los {n_grupos} grupos de traducción</a> · <a href="en-camino/">traducciones en camino</a> · <a href="parchear/#mi-carpeta">escanear tu carpeta de juegos</a> en el navegador.</p>
  </section>''' if portada_sels else '')
    cuerpo = f'''<main>
  <section class="heroe {'mosaico-h' if mosaico else 'uno'}">
    <div class="heroe-t">
      <h1>Juega en español a juegos que nunca salieron traducidos</h1>
      <p class="lede">{n_trad:,} traducciones hechas por aficionados de España y de toda Latinoamérica, de la NES a la Switch y los juegos de PC de hoy, en un solo catálogo. Cada ficha te lleva a la traducción y te dice qué copia del juego pide; muchas se aplican en el navegador o con el programa en un clic.</p>
      <form class="buscador" role="search" action="catalogo/" method="get">
        <label class="sr" for="q">Buscar un juego</label>
        <input id="q" name="q" type="search" placeholder="¿Qué juego quieres jugar en español?" autocomplete="off">
        <button class="btn primario grande" type="submit">Buscar</button>
      </form>
      <div class="acciones">
        <a class="btn" href="catalogo/">Todo el catálogo</a>
        <a class="btn" href="parchear/#mi-carpeta">Escanear mi carpeta de juegos</a>
        <a class="btn" href="{INSTALADOR}" data-goatcounter-click="descarga-portada">Programa para Windows</a>
      </div>
      <p class="suave nota-var">Traducciones en español de España, latino y neutro: cuando sus autores indican la variante, la ficha lo dice.</p>
    </div>
    {'<div class="mosaico" aria-label="Portadas de juegos con traducción">' + mosaico + '</div>' if mosaico else ''}
  </section>

  <div id="favoritos-inicio"></div>

  {bloque_nov}

  {bloque_vistos}

  {bloque_dob}

  {bloque_explora}

  <section class="cifras" aria-label="El catálogo en cifras">
    <div><b>{n_trad:,}</b><span>traducciones</span></div>
    <div><b>{n_juegos:,}</b><span>juegos</span></div>
    <div><b>{len(por_sis)}</b><span>sistemas</span></div>
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
    <p class="texto">Las traducciones son de sus autores. El catálogo se ha hecho con los listados de romhacking.net, Traducciones Wave, Romhack Plaza, Romhack Hispano, El Otro Lado, Steam Community, RetroAchievements, Parches en Castellano y blogs y repositorios de traductores. Cada ficha lleva a la página de su traducción, y la web y el programa solo descargan los parches de los sitios que lo permiten.</p>
    <p class="texto">No se enlazan los sitios que reparten juegos ya parcheados o completos. Si eres autor de una traducción y quieres corregir su ficha o que no aparezca, <a href="{INCIDENCIAS}">abre una incidencia</a>.</p>
  </section>

  <section class="bloque preguntas">
    <h2>Preguntas</h2>
    <details><summary>¿Solo hay traducciones en español de España?</summary><p>No. El catálogo recoge todas las traducciones al español que encuentra, hechas en España y en Latinoamérica, y cuando sus autores indican la variante (español de España, latino o neutro) la ficha lo dice; en el catálogo puedes filtrar por «Español latino». Si conoces una traducción latina que falta, <a href="{INCIDENCIAS}">avísame</a>.</p></details>
    <details><summary>¿Hace falta instalar algo?</summary><p>No. La web busca en el mismo catálogo que el programa, cada ficha enlaza a la traducción y la página <a href="parchear/">Parchear</a> aplica los parches en el navegador sin subir nada a ningún sitio. El programa para Windows añade buscar tus juegos él solo, comprobar tu copia por su CRC32, portadas y listas, y el botón Jugar con RetroBat.</p></details>
    <details><summary>¿Descarga juegos?</summary><p>No. Ni la web ni el programa descargan juegos: solo los parches de traducción, que se aplican sobre la copia que ya tienes.</p></details>
    <details><summary>¿Cuánto cuesta?</summary><p>Nada. Es gratuito y de código libre, con licencia GPL-3.0.</p></details>
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
    desc = (f'{n_trad:,} traducciones al español de juegos hechas por fans, de la NES a la Switch y el PC. '
            'Parchea tu ROM gratis en el navegador o con el programa para Windows.')
    # «Biblioteca de traducciones» a secas es también el nombre de dos portales de la Biblioteca Virtual Miguel de
    # Cervantes; el nombre alternativo ayuda a Google a no confundirlos al poner el nombre de la web en los resultados.
    ld = {'@context': 'https://schema.org', '@graph': [
        {'@type': 'WebSite', 'url': base, 'name': SITIO, 'alternateName': ['Biblioteca de traducciones de juegos', 'bibliotecadetraducciones.com'],
         'inLanguage': 'es',
         'potentialAction': {'@type': 'SearchAction', 'target': {'@type': 'EntryPoint', 'urlTemplate': base + 'catalogo/?q={q}'},
                             'query-input': 'required name=q'}},
        {'@type': 'SoftwareApplication', 'name': 'Biblioteca de traducciones', 'url': base, 'description': desc, 'inLanguage': 'es',
         'operatingSystem': 'Windows 10, Windows 11', 'applicationCategory': 'UtilitiesApplication', 'softwareVersion': version,
         'downloadUrl': INSTALADOR, 'license': 'https://www.gnu.org/licenses/gpl-3.0.html', 'image': base + 'img/vista-previa.png',
         'offers': {'@type': 'Offer', 'price': '0', 'priceCurrency': 'EUR'}}]}
    return pagina(raiz, 'Biblioteca de traducciones: juegos traducidos al español por fans', desc, '', cuerpo, base, ld=ld)


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
    return pagina(raiz, 'Novedades: últimas traducciones de juegos al español', desc, 'novedades/', cuerpo, base, ld=ld)


def pagina_doblajes(filas, sysn, base):
    """Los doblajes al español hechos por aficionados: primero los que ponen voces a juegos que ya tenían los textos en
    español y luego las traducciones que también doblan; en cada grupo, los de más tirón primero."""
    raiz = '../'
    # Un juego por apartado, con la traducción de ese tipo que más tirón tiene (Sonic Adventure 2 de PC sale en los dos:
    # el mod de Ecos del doblaje solo pone voces y el de kawaiikaorichan también traduce).
    por_tipo = {'doblaje': {}, 'ambos': {}}
    for f in filas:
        t = doblaje(f)
        if t:
            actual = por_tipo[t].get(id(f['gr']))
            if not actual or f['pop'] > actual['pop']:
                por_tipo[t][id(f['gr'])] = f
    juegos = {id(f['gr']): f for d in por_tipo.values() for f in d.values()}

    def lista(fs):
        fs = sorted(fs, key=lambda f: (-f['pop'], sin_acentos(f['gr']['titulo']).lower()))
        return '<ul class="tarjetas">' + ''.join(tarjeta_juego(f['gr'], raiz, sysn, True, fila=f) for f in fs) + '</ul>'
    solo, ambos = list(por_tipo['doblaje'].values()), list(por_tipo['ambos'].values())
    latinos = sum(1 for f in juegos.values() if variante(f) == 'latino')
    cuerpo = f'''<main class="sistema doblajes-p">
  <p class="migas"><a href="{raiz}">Inicio</a> › Doblajes</p>
  <h1>Doblajes al español</h1>
  <p class="lede">{len(juegos)} juegos con voces en español gracias a grupos de aficionados{f", {latinos} de ellos en español latino" if latinos else ""}. Unos doblan juegos que ya tenían los textos en español; otros traducen y doblan a la vez. Cada ficha dice qué pide el doblaje y te lleva a su página.</p>
  <section class="bloque">
    <h2>Traducciones con doblaje <span class="n">{len(ambos)}</span></h2>
    <p class="texto suave">Textos y voces en español, del mismo grupo o de dos que trabajan juntos.</p>
    {lista(ambos)}
  </section>
  <section class="bloque">
    <h2>Doblajes de juegos con textos en español <span class="n">{len(solo)}</span></h2>
    <p class="texto suave">El juego ya venía traducido, pero con voces en inglés o en japonés: estos mods le ponen voces en español.</p>
    {lista(solo)}
  </section>
  <p class="suave">¿Conoces un doblaje de aficionados que falta? <a href="{INCIDENCIAS}">Avísame en GitHub</a>.</p>
</main>'''
    desc = f'{len(juegos)} juegos con doblaje al español hecho por aficionados: voces en castellano y en latino para juegos de PlayStation, Dreamcast, PC y más.'
    ld = {'@context': 'https://schema.org', '@graph': [migas_ld(base, [('Inicio', ''), ('Doblajes', 'doblajes/')])]}
    return pagina(raiz, 'Doblajes de juegos al español hechos por fans', desc, 'doblajes/', cuerpo, base, ld=ld)


# ---------------------------------------------------------------- grupos de traducción
HOSTS_GENERICOS = {'github.com', 'drive.google.com', 'docs.google.com', 'sites.google.com', 'mega.nz', 'mega.co.nz', 'mediafire.com',
                   'www.mediafire.com', 'archive.org', 'web.archive.org', 'www.romhacking.net', 'romhacking.net', 'steamcommunity.com',
                   'store.steampowered.com', 'tradusquare.es', 'www.elotrolado.net', 'elotrolado.net', 'gamebanana.com', 'www.nexusmods.com',
                   'nexusmods.com', 'itch.io', 'www.abandonsocios.org', 'abandonsocios.org', 'www.dropbox.com', 'dropbox.com', 't.me',
                   'discord.gg', 'discord.com', 'www.youtube.com', 'youtube.com', 'youtu.be', 'twitter.com', 'x.com', 'gofile.io', '1drv.ms',
                   'onedrive.live.com', 'pixeldrain.com', 'www.patreon.com', 'ko-fi.com', 'www.reddit.com', 'reddit.com', 'romhackplaza.org',
                   'www.romhackplaza.org', 'mega.io', 'workupload.com', 'www.4shared.com', 'drive.usercontent.google.com', 'bsky.app'}


def webs_grupo(filas):
    """Las webs propias donde publica un grupo: los dominios que más repiten sus fichas, sin los sitios de descargas y foros."""
    hosts = collections.Counter()
    for f in filas:
        m = re.match(r'https?://([^/?#]+)', web_segura(f['url']))
        if m and m.group(1).lower() not in HOSTS_GENERICOS:
            hosts[m.group(1).lower()] += 1
    return [h for h, n in hosts.most_common(3) if n >= 2 or n * 3 >= len(filas)]


def pagina_grupo(nombre, items, sysn, base):
    """Las traducciones de un grupo, por sistema; en cada sistema, primero las de los juegos con más tirón."""
    raiz = '../../'
    por_sis = {}
    for g, f in items:
        por_sis.setdefault(g['s'], {})
        actual = por_sis[g['s']].get(id(g))
        if not actual or f['pop'] > actual[1]['pop']:
            por_sis[g['s']][id(g)] = (g, f)
    n_juegos = sum(len(v) for v in por_sis.values())
    orden_sis = sorted(por_sis, key=lambda s: (-len(por_sis[s]), sysn.get(s, s)))
    bloques = ''.join(
        f'<section class="letra"><h2>{e(sysn.get(s, s))} <span class="n">{len(por_sis[s])}</span></h2><ul class="tarjetas">'
        + ''.join(tarjeta_juego(g, raiz, sysn, False, fila=f) for g, f in sorted(por_sis[s].values(), key=lambda gf: -gf[1]['pop'])) + '</ul></section>'
        for s in orden_sis)
    webs = webs_grupo([f for _, f in items])
    sistemas = [sysn.get(s, s) for s in orden_sis]
    ia = sum(1 for _, f in items if es_ia(f))
    dob = sum(1 for _, f in items if doblaje(f))
    extra = ([f'{dob} con doblaje'] if dob else []) + ([f'{ia} hechas con IA'] if ia else [])
    titulo = nombre if re.match(r'(?i)traducci|traduc', nombre) else f'Traducciones de {nombre}'
    cuerpo = f'''<main class="sistema grupo-p">
  <p class="migas"><a href="{raiz}">Inicio</a> › <a href="{raiz}grupos/">Grupos de traducción</a> › {e(nombre)}</p>
  <h1>{e(titulo)}</h1>
  <p class="lede">{len(items)} {"traducción" if len(items) == 1 else "traducciones"} al español de {n_juegos} {"juego" if n_juegos == 1 else "juegos"} en el catálogo, para {e(enumera(sistemas) if len(sistemas) <= 6 else ", ".join(sistemas[:6]) + " y otros")}{(" (" + ", ".join(extra) + ")") if extra else ""}.</p>
  {f'<p class="texto">Publica en {enumera([f"""<a href="https://{e(h)}/" target="_blank" rel="noopener">{e(h)}</a>""" for h in webs])}.</p>' if webs else ''}
  <p class="suave">Cada ficha lleva a la página de su traducción. ¿Eres de {e(nombre)} y algo está mal o falta? <a href="{INCIDENCIAS}">Avísame en GitHub</a>.</p>
  {bloques}
</main>'''
    ruta = ruta_grupo(nombre)
    desc = el_mas_largo_que_quepa([f'{len(items):,} traducciones al español de {nombre} para {enumera(sistemas[:k])}. '
                                   'Con los datos de cada una y el enlace a su página.' for k in (4, 3, 2, 1)], MAX_DESC)
    ld = {'@context': 'https://schema.org', '@graph': [migas_ld(base, [('Inicio', ''), ('Grupos de traducción', 'grupos/'), (nombre, ruta)])]}
    juegos_t = f'{n_juegos:,} {"juego traducido" if n_juegos == 1 else "juegos traducidos"} al español'
    titulo_pagina = next((t for t in (f'{titulo}: {juegos_t}', f'{nombre}: {juegos_t}') if len(t) <= MAX_TITULO), titulo)
    return pagina(raiz, titulo_pagina, desc, ruta, cuerpo, base, ld=ld)


def pagina_grupos(grupos, base):
    """El índice de grupos con página: de más a menos traducciones."""
    raiz = '../'
    lista = ''.join(f'<a class="sis" href="{raiz}{ruta_grupo(n)}">{e(n)} <span>{len(items)}</span></a>'
                    for n, items in sorted(grupos.items(), key=lambda kv: (-len(kv[1]), sin_acentos(kv[0]).lower())))
    cuerpo = f'''<main class="sistema grupos-p">
  <p class="migas"><a href="{raiz}">Inicio</a> › Grupos de traducción</p>
  <h1>Grupos de traducción</h1>
  <p class="lede">Los {len(grupos)} grupos y traductores con {MIN_GRUPO} o más traducciones en el catálogo. Son quienes hacen posible jugar en español: cada página reúne sus traducciones y las webs donde publican.</p>
  <div class="sistemas">{lista}</div>
  <p class="suave">Los que tienen menos traducciones salen en la ficha de cada juego. ¿Falta tu grupo o está mal escrito? <a href="{INCIDENCIAS}">Avísame en GitHub</a>.</p>
</main>'''
    desc = f'Los {len(grupos)} grupos de traducción al español con más traducciones de juegos hechas por aficionados, cada uno con sus traducciones.'
    ld = {'@context': 'https://schema.org', '@graph': [migas_ld(base, [('Inicio', ''), ('Grupos de traducción', 'grupos/')])]}
    return pagina(raiz, 'Grupos de traducción de juegos al español', desc, 'grupos/', cuerpo, base, ld=ld)


# ---------------------------------------------------------------- selecciones
def es_japonesa(f):
    """Traducida desde la versión japonesa: la ROM que pide o las notas lo dicen."""
    return bool(re.search(r'\bJapan\b|\(J\)|Jap[oó]n|\bJPN?\b|japonés', f['rom_requerida'] + ' ' + f['notas']))


def de_genero(g, *gs):
    return any(x in g['dj'].get('g', []) for x in gs)


AVENTURAS = {'scummvm', 'dos', 'pc', 'windows', 'pc98', 'amiga', 'megacd', 'pcenginecd'}
# Las «aventuras» de acción, rol o disparos no son aventuras gráficas.
NO_AVENTURA = ('Action', 'Shooter', 'Role-Playing', 'Platform', 'Fighting', 'Racing', 'Sports', 'Strategy', "Beat 'em Up", 'Sandbox', 'Stealth', 'Visual Novel')
SELECCIONES_GENERO = [
    ('rpg-japoneses', 'RPG japoneses en español', 'Juegos de rol traducidos desde su versión japonesa; muchos nunca salieron de Japón.',
     lambda g: de_genero(g, 'Role-Playing') and any(es_japonesa(f) for f in g['filas'])),
    ('rpg', 'Juegos de rol en español', 'Los juegos de rol con más fama que tienen traducción al español hecha por aficionados.',
     lambda g: de_genero(g, 'Role-Playing')),
    ('aventuras-graficas', 'Aventuras gráficas y narrativas en español', 'De Monkey Island y las de Sierra a Danganronpa o The Wolf Among Us: aventuras traducidas al español por aficionados.',
     lambda g: de_genero(g, 'Adventure') and not de_genero(g, *NO_AVENTURA) and g['s'] in AVENTURAS),
    ('novelas-visuales', 'Novelas visuales en español', 'Novelas visuales traducidas al español por aficionados. Las que son para adultos solo salen si las activas en el catálogo.',
     lambda g: de_genero(g, 'Visual Novel')),
    ('terror', 'Terror y survival horror en español', 'Resident Evil, Silent Hill, Clock Tower y otros juegos de miedo, traducidos (y algunos doblados) por aficionados.',
     lambda g: de_genero(g, 'Horror')),
    ('plataformas', 'Plataformas en español', 'Los juegos de plataformas con traducción al español hecha por aficionados.', lambda g: de_genero(g, 'Platform')),
    ('lucha', 'Lucha y beat em up en español', "Juegos de lucha y beat 'em up traducidos por aficionados.", lambda g: de_genero(g, 'Fighting', "Beat 'em Up")),
    ('estrategia', 'Estrategia y táctica en español', 'Juegos de estrategia y de rol táctico traducidos por aficionados.', lambda g: de_genero(g, 'Strategy')),
    ('con-logros', 'Traducciones con logros', 'Juegos traducidos con los que RetroAchievements tiene logros: la ficha dice qué versión del parche guardan en su GitHub.',
     lambda g: any(tiene_logros(f) for f in g['filas'])),
    # Los RPG cuentan como de dos jugadores por el cable link (Pokémon): fuera, igual que las novelas visuales.
    ('para-jugar-acompanado', 'Para jugar acompañado', 'Juegos traducidos al español en los que pueden jugar dos o más personas a la vez.',
     lambda g: (g['dj'].get('j') or '1').isdigit() and int(g['dj'].get('j') or '1') >= 2 and not de_genero(g, 'Role-Playing', 'Visual Novel')),
    ('deportes', 'Deportes en español', 'Fútbol, golf, béisbol, tenis y otros juegos deportivos traducidos al español por aficionados.', lambda g: de_genero(g, 'Sports')),
    ('carreras', 'Carreras en español', 'Juegos de coches y de carreras con traducción al español hecha por aficionados.', lambda g: de_genero(g, 'Racing')),
    ('disparos', "Disparos y shoot 'em up en español", "Matamarcianos, run and gun y juegos de disparos traducidos al español por aficionados.", lambda g: de_genero(g, 'Shooter')),
    ('puzles', 'Puzles en español', 'Juegos de puzles y de ingenio traducidos al español por aficionados.', lambda g: de_genero(g, 'Puzzle')),
    ('simulacion', 'Simulación y gestión en español', 'Granjas, ciudades, parques, aviones y otros juegos de simulación y gestión traducidos por aficionados.',
     lambda g: de_genero(g, 'Life Simulation', 'Construction and Management Simulation', 'Flight Simulator', 'Vehicle Simulation')
     and not de_genero(g, 'Visual Novel', 'Adventure')),
]
# Selecciones temáticas hechas a mano: LaunchBox no dice quién protagoniza un juego, cuánto texto tiene ni si sale de un
# anime, así que van por nombre (el del juego o el de alguna de sus traducciones). Para añadir un juego, su patrón.
TEMAS = [
    ('chicas-al-poder', 'Chicas al poder', 'Juegos protagonizados por mujeres: de Samus Aran y Lara Croft a Laura Bow, Shantae o las guerreras '
     'de Sailor Moon, traducidos al español por aficionados.',
     [r'^(Super )?Metroid\b', r'^Tomb Raider', r'^Clock Tower', r'^Parasite Eve', r'^(FATAL FRAME|Fatal Frame)', r'^Shantae', r'^(Syd of )?Valis\b',
      r'^Wonder Momo', r'^Athena$', r'Sailor Moon', r'^Magic Knight Rayearth', r'^Valkyrie Profile$', r'^Princess Crown', r'^Lady Stalker',
      r'^Atelier ', r'^Night in the Woods', r'^Wild Arms 3', r"^Magical Pop'n", r'^Little Witch in the Woods', r'^WitchSpring',
      r'^Touhou Luna Nights', r"^Heroine's Quest", r'^Miracle Girls', r'^YAWARA', r'Madoka Magica', r'Neptunia|Superdimension Neptune', r'^Arcana Heart',
      r"Gal'?s Fi", r'^Xenosaga Episode I:', r'Little Mermaid', r'^Blackwell', r'^Kathy Rain', r'Perils of Rosella', r'^Kuon$',
      r'^Nancy Drew', r'^Phantasmagoria$', r"Colonel's Bequest", r'Dagger of Amon Ra', r'^Dreamfall']),
    ('mucho-texto', 'Mucho texto', 'Rol, novelas visuales y aventuras en las que se lee muchísimo: los juegos donde más se agradece '
     'una traducción.',
     [r'^Planescape', r'^Disco Elysium', r"^Baldur's Gate", r'^Icewind Dale', r'^Arcanum', r'^Fallout( 2)?(:|$)', r'^Dragon Age',
      r'Knights of the Old Republic', r'^Divinity: Original Sin', r'^Ultima VII', r'^Roadwarden', r'^Citizen Sleeper', r'^Sunless Sea',
      r'^Snatcher', r'^Policenauts', r'^Famicom Detective Club', r'Ace Attorney', r'^Danganronpa', r'^AI: The Somnium', r'^Paranormasight',
      r'^Steins;Gate', r'^(CHAOS|Chaos);', r'^Anonymous;Code', r'^Ever17', r'^CLANNAD', r'^Little Busters', r'^AIR$', r'^Fate/stay night',
      r'^Tsukihime', r'^Higurashi', r'^Umineko', r'^Saya no Uta', r'^The House in Fata Morgana', r'^Kara no Sho', r'^Sharin no Kuni',
      r'Grisaia', r'^Persona [2-5Q]', r'^Shin Megami Tensei: Persona', r'^Suikoden', r'^Chrono Trigger', r'^Mother 3', r'^EarthBound',
      r'^Lunar: The Silver Star', r'^Xenogears', r'^Xenosaga', r'^Tales of (?!Monkey|Maj)', r'^Trails ', r'^Yakuza', r'^Valkyrie Profile',
      r'^Vagrant Story', r'^Final Fantasy Tactics', r'^Tactics Ogre', r'^(UNDERTALE|Undertale|DELTARUNE)', r'^OMORI', r'^Grim Fandango',
      r'^Monkey Island 2', r'^Gabriel Knight', r'^Quest for Glory', r'^Langrisser']),
    ('basados-en-animes', 'Juegos basados en animes', 'Dragon Ball, Saint Seiya, Sailor Moon, Captain Tsubasa, Ranma, Hokuto no Ken y otras '
     'series de anime convertidas en videojuego, traducidas al español por aficionados.',
     [r'Sailor Moon', r'^Dragon Ball', r'Saint Seiya', r'Captain Tsubasa', r'^Ranma', r'Yu ?Yu Hakusho', r'Hokuto no Ken|Fist of the North Star',
      r'Evangelion', r'Gundam', r'Macross', r'^Lupin', r'^Doraemon', r'Kinnikuman|M\.U\.S\.C\.L\.E', r'^Dr\. Slump', r'One Piece', r'^Naruto',
      r'^Digimon Adventure', r'^Magic Knight Rayearth', r'Madoka Magica', r'^Slayers', r'Record of Lodoss War', r'Berserk', r'Shin-?Chan',
      r'Chibi Maruko', r'^JoJo', r'^City Hunter', r'Urusei Yatsura', r'Patlabor', r'^Mazinger', r'Super Robot (Taisen|Wars)', r'^Shaman King',
      r'Hajime no Ippo', r'^Attack on Titan', r'^Ashita no Joe', r'^Bastard!!', r'^Devilman', r'^Dirty Pair', r'Gegege no Kit',
      r'Hattori-kun', r'^YAWARA', r'^Tenchi Muy', r'^Yaiba', r'^Maison Ikkoku']),
]
# Sagas: el nombre del juego (o el de alguna de sus traducciones) casa con la expresión.
SAGAS = [
    ('final-fantasy', 'Final Fantasy', r'\bFinal Fantasy\b'),
    ('dragon-quest', 'Dragon Quest', r'\bDragon (?:Quest|Warrior)\b'),
    ('shin-megami-tensei', 'Shin Megami Tensei y Persona', r'\b(?:Megami Tensei|Persona [1-5Q]|Persona:|Devil Summoner|Devil Survivor|Majin Tensei)'),
    ('tales-of', 'Tales of', r'^Tales of (?!Monkey)'),
    ('mega-man', 'Mega Man', r'\b(?:Mega ?Man|Rockman)\b'),
    ('castlevania', 'Castlevania', r'\b(?:Castlevania|Akumajou Dracula)\b'),
    ('zelda', 'The Legend of Zelda', r'\bZelda\b'),
    ('mario', 'Mario', r'\bMario\b'),
    ('sonic', 'Sonic', r'\bSonic\b(?! Blast Man)'),
    ('dragon-ball', 'Dragon Ball', r'\bDragon Ball\b'),
    ('captain-tsubasa', 'Captain Tsubasa', r'\bCaptain Tsubasa\b'),
    ('pokemon', 'Pokémon', r'\bPok[eé]mon\b'),
    ('fire-emblem', 'Fire Emblem', r'\bFire Emblem\b'),
    ('resident-evil', 'Resident Evil', r'\b(?:Resident Evil|Biohazard)\b'),
    ('star-wars', 'Star Wars', r'\bStar Wars\b'),
    ('phantasy-star', 'Phantasy Star', r'\bPhantasy Star\b'),
]
MAS_VISTOS_DIAS = 30


def visitas_fichas():
    """Visitas a cada ficha de juego en los últimos MAS_VISTOS_DIAS días, de la API de GoatCounter: {ruta: visitas}.
    Hace falta una clave de solo lectura de estadísticas en la variable GOATCOUNTER_TOKEN (en GitHub, el secreto del
    mismo nombre que le pasa web.yml). Sin clave, o si la API falla, devuelve {} y «Lo más visto» no sale: la web se
    genera igual."""
    clave = os.environ.get('GOATCOUNTER_TOKEN', '').strip()
    if not (clave and GOATCOUNTER):
        return {}
    import urllib.error, urllib.request
    from datetime import datetime, timedelta, timezone
    from urllib.parse import urlencode
    # La API pide fecha y hora (RFC 3339), redondeadas a la hora.
    fin = datetime.now(timezone.utc).replace(minute=0, second=0, microsecond=0)
    inicio = fin - timedelta(days=MAS_VISTOS_DIAS)
    visitas, vistos = {}, []
    for _ in range(5):  # páginas de 100 rutas, de más visitas a menos
        q = {'start': inicio.strftime('%Y-%m-%dT%H:%M:%SZ'), 'end': fin.strftime('%Y-%m-%dT%H:%M:%SZ'), 'limit': 100}
        if vistos:
            q['exclude_paths'] = ','.join(map(str, vistos))
        pet = urllib.request.Request(f'https://{GOATCOUNTER}.goatcounter.com/api/v0/stats/hits?{urlencode(q)}',
                                     headers={'Authorization': f'Bearer {clave}', 'Content-Type': 'application/json',
                                              'User-Agent': 'BibliotecaTraducciones-web'})
        try:
            with urllib.request.urlopen(pet, timeout=30) as r:
                datos = json.load(r)
        except Exception as ex:  # nunca se enseña la clave: solo el código y el mensaje de error de la API
            motivo = type(ex).__name__
            if isinstance(ex, urllib.error.HTTPError):
                try:
                    cuerpo = json.loads(ex.read().decode('utf-8', 'replace') or '{}')
                    detalle = cuerpo.get('error') or cuerpo.get('errors') or ''
                except Exception:
                    detalle = ''
                motivo = f'HTTP {ex.code} {str(detalle)[:200]}'.strip()
            print(f'AVISO: no se pudieron leer las visitas de GoatCounter ({motivo}); se sigue con lo leído', file=sys.stderr)
            break
        for h in datos.get('hits') or []:
            if h.get('path_id'):
                vistos.append(h['path_id'])
            ruta = (h.get('path') or '').split('?')[0].strip('/')
            if not h.get('event') and ruta.startswith('juego/'):
                visitas[ruta + '/'] = visitas.get(ruta + '/', 0) + int(h.get('count') or 0)
        if not datos.get('more'):
            break
    print(f'visitas de GoatCounter: {len(visitas)} fichas de juego con visitas en {MAS_VISTOS_DIAS} días', file=sys.stderr)
    return visitas


SISTEMAS_IMPRESCINDIBLES = ['snes', 'nes', 'psx', 'megadrive', 'gba', 'gb', 'n64', 'ps2', 'nds', 'psp', 'saturn', 'dreamcast', 'mastersystem', 'pcengine', 'pc']
MAX_SELECCION = 48


def selecciones(juegos, sysn, visitas=None):
    """Las selecciones: [(slug, título, entradilla, [juegos])], de más tirón a menos, sin los juegos que solo tienen traducciones con IA.
    «Lo más visto» va la primera y ordena por las visitas a la ficha (visitas_fichas), con todos los juegos."""
    validos = [g for g in juegos if not all(es_ia(f) for f in g['filas']) and any(f['pop'] for f in g['filas'])]
    tiron = lambda g: -max(f['pop'] for f in g['filas'])
    out = []
    if visitas:
        gs = sorted((g for g in juegos if visitas.get(g['ruta'], 0) >= 2), key=lambda g: (-visitas[g['ruta']], tiron(g) if any(f['pop'] for f in g['filas']) else 0))
        if len(gs) >= 8:
            out.append(('lo-mas-visto', 'Lo más visto', f'Los juegos cuyas fichas más se han consultado en la web en los últimos {MAS_VISTOS_DIAS} días.',
                        gs[:MAX_SELECCION]))
    for s in SISTEMAS_IMPRESCINDIBLES:
        nombre = sysn.get(s, s)
        gs = sorted((g for g in validos if g['s'] == s), key=tiron)[:MAX_SELECCION]
        if len(gs) >= 12:
            out.append((f'imprescindibles-{slug(nombre)}', f'Imprescindibles de {nombre} en español',
                        f'Los juegos de {nombre} con más fama que tienen traducción al español hecha por aficionados.', gs))
    for sl, titulo, intro, filtro in SELECCIONES_GENERO:
        gs = sorted((g for g in validos if filtro(g)), key=tiron)
        if sl == 'con-logros':
            gs = sorted((g for g in juegos if filtro(g)), key=tiron)
        if len(gs) >= 8:
            out.append((sl, titulo, intro, gs[:MAX_SELECCION] if sl != 'con-logros' else gs))
    for sl, titulo, intro, patrones in TEMAS:
        rx = re.compile('|'.join(f'(?:{p})' for p in patrones), re.I)
        gs = sorted((g for g in validos if rx.search(g['titulo']) or any(rx.search(f['juego']) for f in g['filas'])), key=tiron)
        if len(gs) >= 8:
            out.append((sl, titulo, intro, gs[:MAX_SELECCION]))
    for sl, nombre, patron in SAGAS:
        rx = re.compile(patron, re.I)
        gs = sorted((g for g in validos if rx.search(g['titulo']) or any(rx.search(f['juego']) for f in g['filas'])), key=tiron)
        if len(gs) >= 8:
            out.append((f'saga-{sl}', f'{nombre} en español', f'Los juegos de {nombre} con traducción al español hecha por aficionados, '
                        'de los más conocidos a los menos.', gs[:MAX_SELECCION]))
    return out


EXPLORA_FIJAS = ('rpg-japoneses', 'aventuras-graficas', 'novelas-visuales', 'terror')


def explora(sels, dia=None):
    """Las ocho selecciones de «Explora» en la portada (dos filas de cuatro): cuatro fijas y, rotando cada día, dos
    imprescindibles de consola, una saga y otra de género. La elección sale de la fecha, así que el mismo día da
    siempre las mismas aunque la web se regenere varias veces."""
    rnd = random.Random((dia or date.today()).toordinal())
    por_slug = {s[0]: s for s in sels}
    fijas = [por_slug[k] for k in EXPLORA_FIJAS if k in por_slug]
    imp = [s for s in sels if s[0].startswith('imprescindibles-')]
    sagas = [s for s in sels if s[0].startswith('saga-')]
    # «RPG» se parece demasiado a «RPG japoneses», que es fija.
    generos = [s for s in sels if s not in fijas and not s[0].startswith(('imprescindibles-', 'saga-')) and s[0] not in ('lo-mas-visto', 'rpg')]
    rotan = rnd.sample(imp, min(2, len(imp))) + rnd.sample(sagas, min(1, len(sagas))) + rnd.sample(generos, min(1, len(generos)))
    # Intercaladas: una que rota, una fija…; si faltan, se completa con las que quedan.
    out = [s for par in zip(rotan, fijas) for s in par] + rotan[len(fijas):] + fijas[len(rotan):]
    resto = [s for s in imp + sagas + generos if s not in out]
    return (out + rnd.sample(resto, max(0, min(8 - len(out), len(resto)))))[:8]


def tarjeta_seleccion(sel, raiz, href=None, sub=None, evitar=None):
    """Una selección como tarjeta: tres portadas (con las alternativas de sig() por si una falla), el título y cuántos juegos.
    Con evitar (un conjunto que se va llenando), no repite la portada de un juego que ya salió en otra tarjeta."""
    sl, titulo, intro, gs = sel
    ports = ''
    elegidos = [g for g in gs if portadas(g, LIBRETRO, raiz) and not juego_adulto(g)
                and (evitar is None or sin_acentos(g['titulo']).lower() not in evitar)][:3]
    if evitar is not None:
        evitar.update(sin_acentos(g['titulo']).lower() for g in elegidos)
    for g in elegidos:
        u = portadas(g, LIBRETRO, raiz)
        ports += (f'<img loading="lazy" src="{e(u[0])}" data-alt="{e(" ".join(u[1:]))}" alt="" referrerpolicy="no-referrer" '
                  f'onerror="sig(this)">')
    return (f'<a class="sel" href="{href or f"{raiz}selecciones/{e(sl)}/"}"><span class="sel-port">{ports}</span>'
            f'<b>{e(titulo)}</b><span class="suave">{e(sub) if sub else f"{len(gs)} juegos"}</span></a>')


def pagina_seleccion(sel, sysn, base):
    raiz = '../../'
    sl, titulo, intro, gs = sel
    imprescindible = sl.startswith('imprescindibles-')
    cuerpo = f'''<main class="sistema seleccion-p">
  <p class="migas"><a href="{raiz}">Inicio</a> › <a href="{raiz}selecciones/">Selecciones</a> › {e(titulo)}</p>
  <h1>{e(titulo)}</h1>
  <p class="lede">{e(intro)}</p>
  <ul class="tarjetas">{''.join(tarjeta_juego(g, raiz, sysn, not imprescindible) for g in gs)}</ul>
  <p class="suave">{f"Ordenados por las visitas a su ficha en los últimos {MAS_VISTOS_DIAS} días (estadísticas anónimas y sin cookies de GoatCounter). Se pone al día cada día. " if sl == "lo-mas-visto" else "Ordenados por fama (votos en LaunchBox y reseñas en Steam). "}{"" if sl in ("con-logros", "lo-mas-visto") else "No salen los juegos cuyas únicas traducciones están hechas con IA. "}Los juegos para adultos solo aparecen si los activas en el catálogo.</p>
</main>'''
    desc = el_mas_largo_que_quepa([f'{intro} {len(gs)} juegos, cada uno con su ficha y el enlace a su traducción.', intro], MAX_DESC)
    ld = {'@context': 'https://schema.org', '@graph': [
        migas_ld(base, [('Inicio', ''), ('Selecciones', 'selecciones/'), (titulo, f'selecciones/{sl}/')]),
        {'@type': 'ItemList', 'name': titulo, 'numberOfItems': len(gs),
         'itemListElement': [{'@type': 'ListItem', 'position': i + 1, 'url': base + g['ruta'], 'name': g['titulo']} for i, g in enumerate(gs[:20])]}]}
    # Para el buscador, «los mejores …»: se busca mucho «mejores juegos de SNES en español» o «mejores RPG traducidos».
    sistema = re.fullmatch(r'Imprescindibles de (.+) en español', titulo)
    if sl == 'con-logros':
        opciones = [f'{len(gs)} juegos traducidos al español con logros de RetroAchievements', titulo]
    elif sistema:
        opciones = [f'Los {len(gs)} mejores juegos de {sistema.group(1)} en español', titulo]
    elif sl.startswith('saga-'):
        opciones = [f'{titulo}: {len(gs)} juegos traducidos', titulo]
    elif sl == 'lo-mas-visto':
        opciones = [f'Lo más visto: los {len(gs)} juegos traducidos más consultados', titulo]
    elif sl == 'para-jugar-acompanado':
        opciones = [f'{len(gs)} juegos en español para jugar acompañado', titulo]
    else:
        opciones = [f'{titulo}: {"las" if sl in ("aventuras-graficas", "novelas-visuales") else "los"} {len(gs)} mejores', titulo]
    titulo_pagina = next((t for t in opciones if len(t) <= MAX_TITULO), titulo)
    return pagina(raiz, titulo_pagina, desc, f'selecciones/{sl}/', cuerpo, base, ld=ld)


def pagina_selecciones(sels, base, doblados=()):
    raiz = '../'
    sistemas = [s for s in sels if s[0].startswith('imprescindibles-')]
    sagas = [s for s in sels if s[0].startswith('saga-')]
    vistos = [s for s in sels if s[0] == 'lo-mas-visto']
    otras = [s for s in sels if not s[0].startswith(('imprescindibles-', 'saga-')) and s[0] != 'lo-mas-visto']
    cuerpo = f'''<main class="sistema selecciones-p">
  <p class="migas"><a href="{raiz}">Inicio</a> › Selecciones</p>
  <h1>Selecciones</h1>
  <p class="lede">Juegos con traducción al español agrupados por género, por saga y por sistema, de los más conocidos a los menos. Para descubrir qué jugar.</p>
  {f'<section class="bloque"><h2>Lo que más se consulta</h2><div class="sels">{"".join(tarjeta_seleccion(s, raiz) for s in vistos)}</div></section>' if vistos else ''}
  <section class="bloque"><h2>Por género</h2><div class="sels">{''.join(tarjeta_seleccion(s, raiz) for s in otras)}{tarjeta_seleccion(('doblajes', 'Doblajes al español', '', doblados), raiz, href=f'{raiz}doblajes/', sub=f'{len(doblados)} juegos con voces en español') if doblados else ''}</div></section>
  {f'<section class="bloque"><h2>Por saga</h2><div class="sels">{"".join(tarjeta_seleccion(s, raiz) for s in sagas)}</div></section>' if sagas else ''}
  <section class="bloque"><h2>Imprescindibles por sistema</h2><div class="sels">{''.join(tarjeta_seleccion(s, raiz) for s in sistemas)}</div></section>
</main>'''
    desc = 'Selecciones de juegos con traducción al español: RPG japoneses, aventuras gráficas, terror, sagas como Final Fantasy o Mega Man e imprescindibles de cada consola.'
    ld = {'@context': 'https://schema.org', '@graph': [migas_ld(base, [('Inicio', ''), ('Selecciones', 'selecciones/')])]}
    return pagina(raiz, 'Selecciones de juegos traducidos al español', desc, 'selecciones/', cuerpo, base, ld=ld)


# ---------------------------------------------------------------- traducciones en camino
EN_CURSO = {'beta', 'parcial', 'en curso', 'en desarrollo', 'en actualización'}


def pagina_en_camino(juegos, sysn, base):
    """Lo que está por llegar: los proyectos en curso de TraduSquare (web/en_camino.json, de herramientas\\mantenimiento\\en_camino.py)
    y las traducciones del catálogo que aún están a medias (beta, parcial, en curso)."""
    raiz = '../'
    ruta = os.path.join(WEB, 'en_camino.json')
    datos = json.load(open(ruta, encoding='utf-8')) if os.path.isfile(ruta) else {'proyectos': [], 'fecha': ''}
    por_titulo = {}
    for g in juegos:
        por_titulo.setdefault(' '.join(tokens(g['titulo'])), g)
    proys = ''
    for p in datos['proyectos']:
        url = f'https://tradusquare.es/proyectos/{p["slug"]}/'
        ya = por_titulo.get(' '.join(tokens(p['n'])))
        a18 = ' data-a18="1"' if p.get('x') else ''
        proys += (f'<li class="proy"{a18}><a href="{e(url)}" target="_blank" rel="noopener"><b>{e(p["n"])}</b></a>'
                  f'<span class="suave">{e(", ".join(p["p"]))} · {e(p["g"] or "grupo sin indicar")}'
                  + (f' · actualizado el {fecha_corta(p["m"])} de {p["m"][:4]}' if p.get('m') else '')
                  + (f' · <a href="{raiz}{ya["ruta"]}">ya hay otra en el catálogo</a>' if ya else '') + '</span></li>')
    a_medias = []
    for g in juegos:
        fs = [f for f in g['filas'] if f['estado'].strip().lower() in EN_CURSO]
        if fs:
            f = max(fs, key=lambda f: (fecha_iso(f), f['pop']))
            a_medias.append((g, f))
    a_medias.sort(key=lambda gf: (fecha_iso(gf[1]), gf[1]['pop']), reverse=True)
    cuerpo = f'''<main class="sistema en-camino-p">
  <p class="migas"><a href="{raiz}">Inicio</a> › Traducciones en camino</p>
  <h1>Traducciones en camino</h1>
  <p class="lede">Lo que están traduciendo ahora mismo los grupos de aficionados, y las traducciones que ya se pueden jugar aunque aún no estén terminadas.</p>
  {f"""<section class="bloque"><h2>En curso en TraduSquare <span class="n">{len(datos['proyectos'])}</span></h2>
    <p class="texto suave">Proyectos en curso según el listado de TraduSquare del {fecha_larga(datos['fecha'])}. Cada uno lleva a su página, donde el grupo cuenta cómo va.</p>
    <ul class="proys">{proys}</ul></section>""" if proys else ''}
  <section class="bloque"><h2>Se pueden jugar, pero están a medias <span class="n">{len(a_medias)}</span></h2>
    <p class="texto suave">Traducciones del catálogo en beta, parciales o en curso, de la más reciente a la más antigua.</p>
    <ul class="tarjetas">{''.join(tarjeta_juego(g, raiz, sysn, True, fila=f, extra=f' · {e(f["estado"].strip().lower())}') for g, f in a_medias)}</ul></section>
</main>'''
    desc = 'Traducciones al español de juegos que están en camino: proyectos en curso de los grupos de aficionados y traducciones en beta que ya se pueden jugar.'
    ld = {'@context': 'https://schema.org', '@graph': [migas_ld(base, [('Inicio', ''), ('Traducciones en camino', 'en-camino/')])]}
    return pagina(raiz, 'Traducciones de juegos al español en camino', desc, 'en-camino/', cuerpo, base, ld=ld)


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
    # Sin contenido propio (todo sale del localStorage de cada uno): fuera de los buscadores.
    return pagina(raiz, f'Mis listas · {SITIO}', 'Tus juegos favoritos y tus listas de traducciones, guardados en tu navegador.', 'mis-listas/', cuerpo, base,
                  indexar=False)


def pagina_404(base):
    cuerpo = f'''<main class="sistema">
  <h1>Esta página no existe</h1>
  <p class="lede">Puede que el juego haya cambiado de nombre en el catálogo. Búscalo en el <a href="{base}catalogo/">catálogo</a> o vuelve al <a href="{base}">inicio</a>.</p>
</main>'''
    return pagina(base, f'Página no encontrada · {SITIO}', 'Esta página no existe.', '404.html', cuerpo, base, indexar=False)


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
<h1>Parchear ROMs en el navegador</h1>
<p class="lede">Elige tu ROM o toda tu carpeta de juegos: la página reconoce cada juego por su CRC32, baja el parche de la web de sus autores y te da el juego traducido. Todo ocurre en tu navegador: tus juegos no se suben a ningún sitio.</p>
<div class="zona" id="zona">
  <input type="file" id="rom" aria-label="Tu ROM">
  <p>Arrastra aquí tu ROM (o un zip con ella) o <label for="rom" class="btn">elige el fichero</label></p>
</div>
<div class="zona carpeta" id="mi-carpeta">
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
<section class="bloque como">
  <h2>Cómo parchear una ROM al español</h2>
  <ol class="pasos">
    <li><b>Elige tu ROM</b> (o un zip con ella) o toda tu carpeta de juegos. La página calcula su CRC32, la huella que distingue cada versión de un juego, con y sin la cabecera de 16 bytes de las ROMs de NES y la de 512 bytes de copiador de las de Super Nintendo.</li>
    <li><b>Elige la traducción.</b> Si el catálogo tiene alguna para esa versión exacta, la página baja el parche de la web de sus autores. Si no puede bajarlo, o ya tienes tú el parche, cárgalo con «Tengo el parche».</li>
    <li><b>Guarda el juego traducido.</b> Es una copia nueva con el parche aplicado: tu ROM original no se toca.</li>
  </ol>
</section>
<section class="bloque">
  <h2>Formatos de parche</h2>
  <p class="texto">Aplica parches IPS, BPS, UPS, PPF (versiones 1 a 3) y APS (de Nintendo 64 y de Game Boy Advance), sueltos o dentro de un zip, con las mismas comprobaciones que el programa para Windows. Los parches xdelta, muy usados en los juegos en CD, y los juegos en 7z, rar o CHD necesitan el <a href="{raiz}#descargar">programa</a>.</p>
</section>
<section class="bloque preguntas">
  <h2>Preguntas</h2>
  <details><summary>¿Y si no encuentra traducción para mi ROM?</summary><p>Cada traducción se hace sobre una versión concreta del juego (región y revisión), y la página busca la que casa con el CRC32 de la tuya. Si no hay ninguna, puede que la haya para otra revisión: busca el juego en el <a href="{raiz}catalogo/">catálogo</a>, cuya ficha dice qué versión pide cada traducción. Si ya tienes un parche, puedes aplicarlo igualmente con «Tengo el parche».</p></details>
  <details><summary>¿Se sube mi juego a algún sitio?</summary><p>No. Todo ocurre en tu navegador: la página solo descarga el parche de traducción, y el juego traducido se guarda en tu equipo.</p></details>
  <details><summary>¿Descarga juegos?</summary><p>No. Necesitas tu propia copia del juego: la página solo aplica sobre ella los parches de traducción de sus autores.</p></details>
  <details><summary>¿Qué juegos tienen traducción?</summary><p>El <a href="{raiz}catalogo/">catálogo</a> tiene traducciones al español hechas por aficionados para NES, Super Nintendo, Game Boy, Mega Drive, PlayStation y muchos sistemas más; las <a href="{raiz}selecciones/">selecciones</a> reúnen las más conocidas de cada uno.</p></details>
</section>
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
    desc = ('Parchea tu ROM online y gratis: la página reconoce el juego, baja su traducción al español y aplica '
            'parches IPS, BPS, UPS, PPF o APS. Nada se sube.')
    ld = {'@context': 'https://schema.org', '@graph': [
        migas_ld(base, [('Inicio', ''), ('Parchear en el navegador', 'parchear/')]),
        {'@type': 'WebApplication', 'name': 'Parchear en el navegador', 'url': base + 'parchear/', 'description': desc, 'inLanguage': 'es',
         'applicationCategory': 'UtilitiesApplication', 'operatingSystem': 'Cualquiera con un navegador moderno', 'browserRequirements': 'Chrome o Edge recomendados',
         'offers': {'@type': 'Offer', 'price': '0', 'priceCurrency': 'EUR'}}]}
    return pagina(raiz, 'Parchear ROM online: traducciones al español en el navegador', desc, 'parchear/', cuerpo, base, ld=ld)


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
    # Juegos distintos con el mismo título en el mismo sistema: su ficha lleva el año en el título de la página.
    repetidos = {k for k, v in collections.Counter((g['titulo'], g['s']) for g in juegos).items() if v > 1}
    ctx = {'sysn': sysn, 'por_sis': por_sis, 'por_grupo': por_grupo, 'repetidos': repetidos}
    # Grupos con página propia (cada parte de «A / B» cuenta para los dos) y su ruta, sin repetir.
    por_parte = {}
    for g in juegos:
        for f in g['filas']:
            for p in dict.fromkeys(partes_grupo(f['grupo'])):
                por_parte.setdefault(p, []).append((g, f))
    grupos = {n: items for n, items in por_parte.items() if len(items) >= MIN_GRUPO}
    GRUPOS_CON_PAGINA.update(grupos)
    for n in sorted(grupos, key=lambda n: (-len(grupos[n]), n)):
        base_slug, k = slug(n) or 'grupo', 2
        ruta = f'grupo/{base_slug}/'
        while ruta in RUTAS_GRUPO.values():
            ruta = f'grupo/{base_slug}-{k}/'
            k += 1
        RUTAS_GRUPO[n] = ruta
    sels = selecciones(juegos, sysn, visitas_fichas())
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
    # El icono de la web (web/icono.svg, sin bandera) o, si no está, el del programa.
    icono = os.path.join(WEB, 'icono.svg')
    shutil.copy(icono if os.path.isfile(icono) else os.path.join(RAIZ, 'ui', 'icono.svg'), os.path.join(salida, 'icono.svg'))
    shutil.copy(os.path.join(WEB, 'estilo.css'), os.path.join(salida, 'estilo.css'))
    if os.path.isfile(os.path.join(WEB, 'estadisticas.js')):
        shutil.copy(os.path.join(WEB, 'estadisticas.js'), os.path.join(salida, 'estadisticas.js'))
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
    escribir(salida, 'index.html', miles(pagina_inicio(por_sis, sysn, base, n_trad, n_juegos, version, dias_publicas, filas, fijados(), sels, len(grupos))))
    escribir(salida, 'catalogo/index.html', miles(pagina_catalogo(por_sis, sysn, base, n_trad, n_juegos, filas)))
    escribir(salida, 'novedades/index.html', miles(pagina_novedades(dias, filas, sysn, base)))
    escribir(salida, 'novedades.xml', feed_novedades(dias_publicas, sysn, base))
    escribir(salida, 'doblajes/index.html', miles(pagina_doblajes(filas, sysn, base)))
    escribir(salida, 'grupos/index.html', miles(pagina_grupos(grupos, base)))
    for n, items in grupos.items():
        escribir(salida, ruta_grupo(n) + 'index.html', miles(pagina_grupo(n, items, sysn, base)))
    escribir(salida, 'selecciones/index.html', miles(pagina_selecciones(sels, base, sorted({id(f['gr']): f['gr'] for f in filas if doblaje(f)}.values(), key=lambda g: -max(x['pop'] for x in g['filas'])))))
    for sel in sels:
        escribir(salida, f'selecciones/{sel[0]}/index.html', miles(pagina_seleccion(sel, sysn, base)))
    escribir(salida, 'en-camino/index.html', miles(pagina_en_camino(juegos, sysn, base)))
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
    rutas = ([('', hoy), ('catalogo/', hoy), ('novedades/', hoy), ('doblajes/', hoy), ('parchear/', hoy), ('grupos/', hoy),
              ('selecciones/', hoy), ('en-camino/', hoy)]
             + [(f'selecciones/{s[0]}/', hoy) for s in sels]
             + [(ruta_grupo(n), max((fecha_iso(f) for _, f in items), default='')) for n, items in grupos.items()]
             + [(f'sistema/{s}/', max((g['fecha'] for g in gs), default='')) for s, gs in por_sis.items()]
             + [(g['ruta'], g['fecha']) for g in juegos])
    escribir(salida, 'sitemap.xml', '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
             + ''.join(f'<url><loc>{e(base + r)}</loc>' + (f'<lastmod>{d}</lastmod>' if d else '') + '</url>\n' for r, d in rutas) + '</urlset>\n')
    escribir(salida, 'robots.txt', f'User-agent: *\nAllow: /\nSitemap: {base}sitemap.xml\n')
    print(f'Web en {salida}: {n_juegos} juegos, {len(por_sis)} sistemas, {len(rutas)} páginas; novedades: {len(altas)} altas en {len(dias)} días. Dirección: {base}')


if __name__ == '__main__':
    main()
