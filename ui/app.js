'use strict';
// Biblioteca de traducciones: catálogo con portadas, tus juegos (ROMs de tus carpetas y juegos de PC
// de Steam, GOG y Epic) emparejados con sus traducciones y la cola de trabajos. Lo que toca disco o
// red lo hace el programa (src-tauri) con invoke().

const T = window.__TAURI__;
const SYS = {nes:'NES',fds:'Famicom Disk System',snes:'Super Nintendo',satellaview:'Satellaview',sufami:'SuFami Turbo',gb:'Game Boy',gbc:'Game Boy Color',gba:'Game Boy Advance',n64:'Nintendo 64',nds:'Nintendo DS','3ds':'Nintendo 3DS',virtualboy:'Virtual Boy',gamecube:'GameCube',wii:'Wii',sg1000:'SG-1000',mastersystem:'Master System',gamegear:'Game Gear',megadrive:'Mega Drive',sega32x:'32X',megacd:'Mega CD',saturn:'Saturn',dreamcast:'Dreamcast',psx:'PlayStation',ps2:'PlayStation 2',psp:'PSP',psvita:'PS Vita',pcengine:'PC Engine',supergrafx:'SuperGrafx',pcenginecd:'PC Engine CD',pcfx:'PC-FX',neogeo:'Neo Geo',neogeocd:'Neo Geo CD',ngp:'Neo Geo Pocket',ngpc:'Neo Geo Pocket Color',wswan:'WonderSwan',wswanc:'WonderSwan Color',msx1:'MSX',msx2:'MSX2',pc98:'PC-98',x68000:'X68000',zxspectrum:'ZX Spectrum',amstradcpc:'Amstrad CPC',c64:'Commodore 64','3do':'3DO',colecovision:'ColecoVision',dos:'MS-DOS',windows:'Windows clásico',scummvm:'ScummVM',easyrpg:'RPG Maker',arcade:'Arcade',lynx:'Atari Lynx',xbox:'Xbox',switch:'Nintendo Switch',ps3:'PlayStation 3',ps4:'PlayStation 4',ps5:'PlayStation 5',wiiu:'Wii U',xbox360:'Xbox 360',xboxone:'Xbox One',pc:'PC'};
const PC = new Set(['dos', 'windows', 'scummvm', 'easyrpg', 'pc']);
// Consolas de hoy: sus traducciones son mods que se instalan a mano, como las de PC.
const MODERNAS = new Set(['switch', 'ps3', 'ps4', 'ps5', 'wiiu', 'xbox360', 'xboxone']);
const A_MANO = new Set([...PC, ...MODERNAS]);
const TIENDA = {steam: 'Steam', gog: 'GOG', epic: 'Epic Games'};
const F = {sistema:0, juego:1, orig:2, grupo:3, version:4, fecha:5, estado:6, fmt:7, rom:8, crc:9, sha1:10, url:11, fuente:12, notas:13};
const FUENTE = {wave: 'Traducciones Wave', rhdn2021: 'la copia de romhacking.net (2021) en archive.org', github: 'GitHub',
                ra: 'la copia de parches de RetroAchievements en GitHub'};
const VISTAS = {inicio: 'Inicio', mios: 'Mis juegos', todas: 'Descubrir', hechas: 'Traducidos', descargas: 'Descargas', ajustes: 'Ajustes'};

let filas = [], grupos = [], porClave = new Map(), porJuego = new Map(), centro = {sistemas: {}, filas: {}}, traducidos = {}, tes = new Map();
let juegos = [], pares = [], paresPorFila = new Map();
let vista = 'inicio', limit = 60, lista = [];
let filtro = {sys: '', directa: false, completa: false, sinia: false, steam: false};
let escaneando = false, trabajos = [], espera = null, ajustes = {}, propias = {};
let misListas = [{id: 'fav', nombre: 'Favoritos', juegos: []}];
const ICONO_FAV = '<svg viewBox="0 0 24 24" class="i-fav"><path d="M12 20s-7.5-4.6-9.3-9.2C1.4 7.4 3.6 4 7 4c2 0 3.6 1.1 5 3 1.4-1.9 3-3 5-3 3.4 0 5.6 3.4 4.3 6.8C19.5 15.4 12 20 12 20z"/></svg>';
const ICONO_LISTA = '<svg viewBox="0 0 24 24"><path d="M4 6h2v2H4zm4 0h12v2H8zM4 11h2v2H4zm4 0h12v2H8zM4 16h2v2H4zm4 0h12v2H8z"/></svg>';
const $ = id => document.getElementById(id);
const main = $('main');
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
// Solo direcciones web en los enlaces que vienen del catálogo (nunca javascript: ni file:).
const webSegura = u => /^https?:\/\/\S+$/i.test(u || '') ? u : '';

async function api(cmd, args, opciones) {
  try {
    const r = await T.core.invoke(cmd, args || {}, opciones);
    return typeof r === 'string' && /^[{\[]/.test(r) ? JSON.parse(r) : r;
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : (e && e.message) || String(e));
  }
}

function aviso(texto, tipo = '') {
  const t = document.createElement('div');
  t.className = 'toast ' + tipo;
  t.textContent = texto;
  // Los errores se anuncian al momento al lector de pantalla y duran más; el resto, con calma.
  t.setAttribute('role', tipo === 'bad' ? 'alert' : 'status');
  // Con una ventana abierta (la ficha, la bienvenida), el aviso va dentro: fuera quedaría detrás.
  const abierta = document.querySelector('dialog[open]');
  const caja = abierta ? abierta.querySelector('.toasts') || abierta.appendChild(Object.assign(document.createElement('div'), {className: 'toasts'})) : $('toasts');
  caja.append(t);
  setTimeout(() => t.remove(), tipo === 'bad' ? 10000 : 6000);
}

// ---------------------------------------------------------------- nombres (como Parchear.ps1)
const STOP = new Set(['the','a','an','el','la','los','las','de','del','of','and','y','en','to','traduccion','traducido','traducida','translation','translated','spanish','espanol','castellano','es','esp','spa','parche','patch','fix','final','rev','beta','completa','completo','ips','bps','ups','ppf','xdelta','aps','rom','iso','track','disc','usa','japan','europe','jap','jp','us','eu']);
const ROMAN = {ii: '2', iii: '3', iv: '4', vi: '6', vii: '7', viii: '8', ix: '9'};
function tokens(t) {
  if (!t) return [];
  const s = t.replace(/\[[^\]]*\]/g, ' ').replace(/\([^)]*\)/g, ' ').normalize('NFD').replace(/[\u0300-\u036f]/g, '')
    .toLowerCase().replace(/['’]/g, '').replace(/[^a-z0-9]+/g, ' ');
  const out = [];
  for (let w of s.split(' ')) {
    if (!w || /^v\d+$/.test(w) || (/^\d+[a-z]?$/.test(w) && w.length > 4) || STOP.has(w)) continue;
    w = ROMAN[w] || w;
    if (!out.includes(w)) out.push(w);
  }
  return out;
}
function simil(a, b) {
  if (!a.length || !b.length) return 0;
  // Secuelas: los números tienen que coincidir ("Mega Man" no es "Mega Man 2").
  const na = a.filter(x => /^\d+$/.test(x)).sort().join(), nb = b.filter(x => /^\d+$/.test(x)).sort().join();
  if (na !== nb) return 0;
  const inter = a.filter(x => b.includes(x)).length;
  if (!inter) return 0;
  const union = new Set([...a, ...b]).size, min = Math.min(a.length, b.length);
  return min >= 2 ? Math.max(inter / union, 0.9 * inter / min) : inter / union;
}

// ---------------------------------------------------------------- datos
function cargarDatos(d) {
  centro = d.centro || {sistemas: {}, filas: {}};
  traducidos = d.traducidos || {};
  filas = d.filas.map((a, i) => {
    const key = a[F.sistema] + '|' + a[F.juego] + '|' + a[F.grupo];
    const x = centro.filas[key] || {};
    // Una descarga puede pedir otra revisión de la ROM que la ficha (p. ej. la de RetroAchievements): también vale.
    const crcs = new Set([...a[F.crc].split('|'), ...(x.d || []).flatMap(d => d.c || [])]
      .map(c => c.trim().toLowerCase()).filter(c => /^[0-9a-f]{8}$/.test(c)));
    const titulos = [a[F.orig], a[F.juego], ...a[F.rom].split('|')].map(tokens).filter(t => t.length);
    return {i, a, key, s: a[F.sistema], crcs, titulos, x};
  });
  porClave = new Map(filas.map(r => [r.key, r]));
  agrupar();
}

// Un juego, una ficha: las traducciones del mismo juego (mismo sistema y mismo título, o mismo AppID de
// Steam) van juntas, y dentro de la ficha se elige cuál.
const conPortada = r => !!(r.x.pm || r.x.p || r.x.steam || r.x.pu);
function claveJuego(r) {
  if (r.x.steam) return 'steam|' + r.x.steam;
  const t = tokens(r.a[F.juego]);
  return r.s + '|' + (t.length ? [...t].sort().join(' ') : r.a[F.juego].toLowerCase());
}
function agrupar() {
  grupos = [];
  porJuego = new Map();
  for (const r of filas) {
    const k = claveJuego(r);
    let gr = porJuego.get(k);
    if (!gr) { gr = {k, s: r.s, filas: []}; porJuego.set(k, gr); grupos.push(gr); }
    gr.filas.push(r);
    r.gr = gr;
  }
  for (const gr of grupos) {
    // El título y la portada del juego: los de una ficha con portada y, si puede ser, sin paréntesis.
    const rep = gr.filas.slice().sort((x, y) => conPortada(y) - conPortada(x) || /\(/.test(x.a[F.juego]) - /\(/.test(y.a[F.juego]))[0];
    gr.titulo = rep.a[F.juego];
    gr.portada = gr.filas.find(r => r.x.pm) || gr.filas.find(conPortada) || rep;
    gr.a18 = gr.filas.every(esAdulto);
  }
}

// ¿Puede la traducción `r` ser para el juego `g` por sistema? Los de PC solo con los de PC; un juego
// cuyo sistema no se sabe (un .bin o un disco en una carpeta sin nombre de sistema) con cualquiera de consola.
const mismoSistema = (g, r) => g.s === 'pc' ? PC.has(r.s) : g.s ? r.s === g.s : !PC.has(r.s);
const anyadir = (m, k, v) => { if (!m.has(k)) m.set(k, []); m.get(k).push(v); };

function emparejar() {
  const porCrc = new Map(), porToken = new Map(), porSteam = new Map();
  for (const r of filas) {
    for (const c of r.crcs) anyadir(porCrc, c, r);
    for (const t of new Set(r.titulos.flat())) anyadir(porToken, t, r);
    if (r.x.steam) anyadir(porSteam, String(r.x.steam), r);
  }
  pares = [];
  for (const g of juegos) {
    const gc = new Set((g.c || []).map(x => x.toLowerCase()));
    const vistosJuego = new Set();
    // El CRC32 dice el juego exacto, esté en la carpeta que esté.
    const exactos = [...[...gc].flatMap(c => porCrc.get(c) || []), ...(g.steam ? porSteam.get(String(g.steam)) || [] : [])];
    for (const r of exactos) {
      if (!vistosJuego.has(r)) { vistosJuego.add(r); pares.push({g, r, tipo: 'exacto'}); }
    }
    const gt = tokens(g.n), cands = new Set();
    for (const t of gt) for (const r of porToken.get(t) || []) if (mismoSistema(g, r)) cands.add(r);
    for (const r of cands) {
      if (vistosJuego.has(r)) continue;
      if (Math.max(...r.titulos.map(t => simil(gt, t))) < 0.8) continue;
      vistosJuego.add(r);
      pares.push({g, r, tipo: r.crcs.size && gc.size ? 'otra' : 'posible'});
    }
  }
  const orden = {exacto: 0, posible: 1, otra: 2};
  pares.sort((p, q) => orden[p.tipo] - orden[q.tipo] || p.g.n.localeCompare(q.g.n));
  paresPorFila = new Map();
  for (const p of pares) { if (!paresPorFila.has(p.r.key)) paresPorFila.set(p.r.key, []); paresPorFila.get(p.r.key).push(p); }
}

const esIA = r => r.a[F.notas].startsWith('[IA]');
// Las de PC y consolas de hoy se instalan a mano aunque su descarga sea directa.
const directa = r => !A_MANO.has(r.s) && (r.x.d || []).length > 0;
const completa = r => /complet/i.test(r.a[F.estado]);
// Juegos para adultos (+18): el catálogo los marca (a18) y van ocultos en todas las secciones hasta que se
// activan en Ajustes; tampoco entran en las recomendaciones. Un juego se oculta cuando todas sus
// traducciones son de adultos.
const esAdulto = r => !!r.x.a18;
let verAdultos = false;
try { verAdultos = localStorage.getItem('adultos') === '1'; } catch (e) { }
function guardarAdultos(v) { verAdultos = v; try { localStorage.setItem('adultos', v ? '1' : '0'); } catch (e) { } }
const visible = gr => verAdultos || !gr.a18;
// Juegos de itch.io (gratuitos o de la tienda indie): fuera de las recomendaciones.
const esItch = r => /itch\.io/i.test([r.a[F.url], r.a[F.fuente], r.x.pj || ''].join(' '));
// Un traducido con la extensión de su sistema (.gg) sale con ese sistema aunque el original (.bin) no lo diga.
const salidaDe = (r, g) => traducidos[r.key] || (g && (tes.get(g.s + '|' + g.n + ' [T-Es]') || tes.get(r.s + '|' + g.n + ' [T-Es]'))) || '';
const hecha = (r, g) => !!salidaDe(r, g);
const mejorPar = r => (paresPorFila.get(r.key) || [])[0] || null;
const ORDEN = {exacto: 0, posible: 1, otra: 2};

// Cuánto conviene enseñar primero una traducción del juego: la ya aplicada, la que vale para tu copia,
// la de un clic, la completa y la más reciente.
function puntos(r) {
  const p = mejorPar(r);
  return (hecha(r, p && p.g) ? 10000 : 0) + (p ? (3 - ORDEN[p.tipo]) * 1000 : 0) + (directa(r) ? 100 : 0) + (completa(r) ? 50 : 0)
    + (esIA(r) ? 0 : 10) + (conPortada(r) ? 1 : 0);
}
const ordenadas = gr => gr.filas.slice().sort((x, y) => puntos(y) - puntos(x) || (y.a[F.fecha] || '').localeCompare(x.a[F.fecha] || ''));

// La traducción que sale en la tarjeta del juego (entre las que pasan los filtros): en una lista, la
// que tenías elegida al guardarlo.
function elegida(gr, vale = () => true, pref = null) {
  const r = pref && pref.gr === gr && vale(pref) ? pref : ordenadas(gr).find(vale);
  return r ? {gr, r, p: mejorPar(r)} : null;
}

// Juegos de cada sección.
function gruposDeTodos(v) {
  if (v === 'mios') {
    const conPar = pares.filter(p => p.tipo !== 'otra' || !pares.some(o => o.g === p.g && o.tipo !== 'otra'));
    return [...new Set(conPar.map(p => p.r.gr))];
  }
  if (v === 'hechas') return grupos.filter(gr => gr.filas.some(r => hecha(r, (mejorPar(r) || {}).g)));
  const l = listaDe(v);
  if (l) return l.juegos.map(j => porJuego.get(j.k)).filter(Boolean);
  return grupos;
}
const gruposDe = v => gruposDeTodos(v).filter(visible);
const itemsDe = v => gruposDe(v).map(gr => elegida(gr, undefined, guardada(listaDe(v), gr)));

function filtrar(grs) {
  const q = tokens($('q').value);
  const vale = r => (!filtro.directa || directa(r)) && (!filtro.completa || completa(r)) && (!filtro.sinia || !esIA(r)) && (!filtro.steam || !!r.x.steam);
  const l = listaDe(vista);
  const out = [];
  for (const gr of grs) {
    if (filtro.sys && gr.s !== filtro.sys) continue;
    if (q.length) {
      const t = tokens(gr.filas.map(r => [r.a[F.juego], r.a[F.orig], r.a[F.grupo], (mejorPar(r) || {g: {n: ''}}).g.n].join(' ')).join(' '));
      if (!q.every(w => t.some(x => x.startsWith(w)))) continue;
    }
    const it = elegida(gr, vale, guardada(l, gr));
    if (it) out.push(it);
  }
  return out;
}

// ---------------------------------------------------------------- listas
// Favoritos y las listas que crees, para volver a esos juegos más tarde. Se guardan en listas.json (en
// la carpeta de datos) con la clave del juego y la traducción que estabas viendo al guardarlo.
const listaDe = v => typeof v === 'string' && v.startsWith('lista:') ? misListas.find(l => l.id === v.slice(6)) || null : null;
const enLista = (l, gr) => !!l && l.juegos.some(j => j.k === gr.k);
const favorito = gr => enLista(listaDe('lista:fav'), gr);
const guardada = (l, gr) => l ? porClave.get((l.juegos.find(j => j.k === gr.k) || {}).t) || null : null;

let listasCambiadas = false;
async function guardarListas() {
  listasCambiadas = true;
  pintarListas();
  try { await api('guardar_listas', {listas: {listas: misListas}}); } catch (e) { aviso(e.message, 'bad'); }
}
// Mete el juego en la lista o lo saca. Devuelve si ha quedado dentro.
function alternar(l, gr, r) {
  const i = l.juegos.findIndex(j => j.k === gr.k);
  if (i >= 0) l.juegos.splice(i, 1);
  else l.juegos.unshift({k: gr.k, t: r ? r.key : '', f: Math.floor(Date.now() / 1000)});
  guardarListas();
  return i < 0;
}
function nuevaLista(nombre) {
  nombre = (nombre || '').trim().slice(0, 80);
  if (!nombre) return null;
  const igual = misListas.find(l => l.nombre.toLowerCase() === nombre.toLowerCase());
  if (igual) return igual;
  const l = {id: 'l' + Date.now().toString(36), nombre, juegos: []};
  misListas.push(l);
  return l;
}

function pintarListas() {
  $('listas-nav').innerHTML = misListas.map(l => `<button type="button" class="nav" data-go="lista:${esc(l.id)}">${l.id === 'fav' ? ICONO_FAV : ICONO_LISTA}<span>${esc(l.nombre)}</span><b class="n">${l.juegos.length || ''}</b></button>`).join('');
  marcarNav();
}

// Corazón de una tarjeta: añade el juego a Favoritos o lo quita.
function alternarFav({gr, r}) {
  const dentro = alternar(listaDe('lista:fav'), gr, r);
  aviso(dentro ? `${gr.titulo}: añadido a Favoritos.` : `${gr.titulo}: quitado de Favoritos.`, 'ok');
  // En Favoritos (o en la fila de Inicio) el juego aparece o desaparece; en el resto solo cambia el corazón.
  if (vista === 'lista:fav' || vista === 'inicio') return pintar();
  main.querySelectorAll('.card').forEach(card => {
    const t = tarjetas[+card.dataset.n];
    if (t && t.gr === gr) marcarFav(card.querySelector('.fav'), dentro);
  });
}
function marcarFav(btn, dentro) {
  btn.setAttribute('aria-pressed', dentro);
  btn.title = btn.ariaLabel = dentro ? 'Quitar de Favoritos' : 'Añadir a Favoritos';
}

// Una pregunta en una ventanita: un nombre (con su campo de texto) o una confirmación. Devuelve el texto
// escrito, true si se confirma o null si se cancela.
function preguntar({titulo, texto = '', valor = null, boton}) {
  const d = $('pregunta');
  $('p-title').textContent = titulo;
  $('p-texto').textContent = texto;
  $('p-texto').hidden = !texto;
  $('p-valor').hidden = valor === null;
  $('p-valor').value = valor || '';
  $('p-si').textContent = boton;
  d.returnValue = '';
  d.showModal();
  if (valor !== null) $('p-valor').select();
  return new Promise(res => d.addEventListener('close', () =>
    res(d.returnValue !== 'si' ? null : valor === null ? true : $('p-valor').value.trim() || null), {once: true}));
}

async function crearLista() {
  const nombre = await preguntar({titulo: 'Lista nueva', texto: 'Ponle un nombre, como «Para este verano» o «RPG japoneses».', valor: '', boton: 'Crear la lista'});
  const l = nuevaLista(nombre);
  if (!l) return;
  await guardarListas();
  ir('lista:' + l.id);
}
async function accionLista(que) {
  const l = listaDe(vista);
  if (!l || l.id === 'fav') return;
  if (que === 'renombrar') {
    const nombre = await preguntar({titulo: 'Cambiar el nombre', valor: l.nombre, boton: 'Guardar'});
    if (!nombre) return;
    l.nombre = nombre.slice(0, 80);
  } else if (que === 'borrar') {
    const si = await preguntar({titulo: `¿Borrar la lista «${l.nombre}»?`, texto: 'Solo se borra la lista: los juegos y sus traducciones siguen en el catálogo.', boton: 'Borrar la lista'});
    if (!si) return;
    misListas = misListas.filter(x => x !== l);
  } else return;
  await guardarListas();
  if (que === 'borrar') ir('inicio'); else pintar();
}

// ---------------------------------------------------------------- portadas
const thumb = (sys, name, tipo) => 'https://thumbnails.libretro.com/' + encodeURIComponent(sys) + '/' + tipo + '/' +
  encodeURIComponent(name.replace(/[&*\/:`<>?\\|"]/g, '_')) + '.png';
// Juegos de PC: la carátula vertical de Steam y, si no la tiene, su cabecera.
const steamImg = (id, f) => `https://cdn.cloudflare.steamstatic.com/steam/apps/${encodeURIComponent(id)}/${f}`;
const REPO_UI = 'https://raw.githubusercontent.com/alucardog-dotcom/biblioteca-traducciones/main/ui/';
const WEB = 'https://bibliotecadetraducciones.com/';
// Enlace de afiliado de Instant Gaming: su búsqueda con el código del autor.
const INSTANT_GAMING = 'https://www.instant-gaming.com/es/busquedas/?query=', AFILIADO = 'gamer-d73144';
// Los juegos de PC que no están en Steam suelen estar en itch.io o en la web de su autor ("pj" en el catálogo).
const sitioPagina = u => /\.itch\.io\//.test(u) ? 'itch.io' : /gamejolt\.com\//.test(u) ? 'Game Jolt' : /store\.steampowered\.com\//.test(u) ? 'Steam'
  : /\.gog\.com\//.test(u) ? 'GOG' : 'la web oficial';
// La que has puesto tú (una imagen copiada a la carpeta de datos o un enlace).
const propia = gr => {
  const m = gr && propias[gr.k];
  return m ? m.url || T.core.convertFileSrc(m.ruta) : '';
};
// La portada del juego: la tuya, la elegida a mano para el catálogo (va dentro del programa, en
// portadas\) o la de cualquiera de sus fichas que la tenga (libretro, Steam o LaunchBox).
function portada(r, done) {
  const c = r.gr ? r.gr.portada : r;
  const sysLib = c.x.ps || centro.sistemas[c.s];
  const tipo = c.x.pt || 'Named_Boxarts', alt = tipo === 'Named_Boxarts' ? 'Named_Titles' : 'Named_Snaps';
  // El título queda debajo mientras la portada se descarga (y si no existe).
  // Se prueban en orden hasta que una carga: la tuya, la del catálogo, la de libretro o Steam, la de
  // LaunchBox y la pantalla de título.
  // La del catálogo va dentro del programa; si es más nueva que el programa (llegó con un catálogo
  // descargado), se trae del repositorio.
  const urls = [propia(r.gr), c.x.pm, c.x.pm && REPO_UI + c.x.pm, ...(c.x.steam ? [steamImg(c.x.steam, 'library_600x900.jpg'), c.x.pu, steamImg(c.x.steam, 'header.jpg')]
    : c.x.p && sysLib ? [thumb(sysLib, c.x.p, tipo), c.x.pu, thumb(sysLib, c.x.p, alt)] : [c.x.pu])];
  const [primera, ...resto] = urls.filter(Boolean);
  let img = `<div class="ph">${esc(r.gr ? r.gr.titulo : r.a[F.juego])}</div>`;
  if (primera) img += `<img loading="lazy" alt="" src="${esc(primera)}" data-alt="${esc(resto.join(' '))}">`;
  return `<div class="cover">${img}<span class="sys">${esc(SYS[r.s] || r.s)}</span>${done ? '<span class="done">Traducido</span>' : ''}<span class="soltar">Suelta la imagen para usarla de portada</span></div>`;
}

// Poner tu portada: eligiendo una imagen, con un enlace, pegándola (Ctrl+V) o soltándola sobre el juego.
async function ponerPortada(gr, cmd, args) {
  try {
    propias = await api(cmd, {clave: gr.k, ...args});
    refrescarPortadas(gr);
    aviso(cmd === 'quitar_portada' ? 'Vuelve a salir la portada automática.' : 'Portada cambiada.', 'ok');
  } catch (e) { aviso(e.message, 'bad'); }
}
async function pegarPortada(gr, fichero) {
  try {
    const datos = new Uint8Array(await fichero.arrayBuffer());
    propias = await api('pegar_portada', datos, {headers: {clave: encodeURIComponent(gr.k)}});
    refrescarPortadas(gr);
    aviso('Portada cambiada.', 'ok');
  } catch (e) { aviso(e.message, 'bad'); }
}
// Cambia la portada en las tarjetas que se ven y en la ficha, sin volver a pintar la lista.
function refrescarPortadas(gr) {
  main.querySelectorAll('.card').forEach(card => {
    const t = tarjetas[+card.dataset.n];
    if (t && t.gr === gr) card.querySelector('.cover').outerHTML = portada(t.r, hecha(t.r, t.p && t.p.g));
  });
  if ($('ficha').open && fichaActual && fichaActual.gr === gr) abrirFicha(fichaActual.r, fichaActual.p);
}
// Las imágenes que se sueltan en la ventana llegan por Tauri (con su ruta), no como un «drop» de la página.
const IMAGEN = /\.(png|jpe?g|webp|gif|bmp|avif)$/i;
let arrastre = null;
function grupoBajo(pos) {
  const el = document.elementFromPoint(pos.x / devicePixelRatio, pos.y / devicePixelRatio);
  const card = el && !$('ficha').open && el.closest('.card');
  if (card) return {gr: tarjetas[+card.dataset.n].gr, cover: card.querySelector('.cover')};
  if ($('ficha').open && fichaActual) return {gr: fichaActual.gr, cover: $('f-body').querySelector('.f-cover .cover')};
  return null;
}
T.webview.getCurrentWebview().onDragDropEvent(({payload: d}) => {
  document.querySelectorAll('.cover.soltando').forEach(c => c.classList.remove('soltando'));
  if (d.type === 'leave') return;
  if (d.type === 'enter') arrastre = (d.paths || []).find(p => IMAGEN.test(p)) || null;
  if (!arrastre) return;
  const bajo = grupoBajo(d.position);
  if (!bajo) return;
  if (d.type === 'drop') { arrastre = null; return ponerPortada(bajo.gr, 'poner_portada', {origen: (d.paths || []).find(p => IMAGEN.test(p))}); }
  if (bajo.cover) bajo.cover.classList.add('soltando');
});
document.addEventListener('paste', e => {
  if (!$('ficha').open || !fichaActual || e.target.closest('input, textarea')) return;
  const img = [...e.clipboardData.items].find(i => i.kind === 'file' && i.type.startsWith('image/'));
  if (img) { e.preventDefault(); return pegarPortada(fichaActual.gr, img.getAsFile()); }
  const texto = e.clipboardData.getData('text').trim();
  if (/^https?:\/\/\S+$/i.test(texto)) { e.preventDefault(); return ponerPortada(fichaActual.gr, 'poner_portada', {origen: texto}); }
});
// La política de seguridad no deja poner onload/onerror en el HTML: se escuchan aquí.
document.addEventListener('load', e => { if (e.target.tagName === 'IMG') e.target.classList.add('ok'); }, true);
document.addEventListener('error', e => {
  const img = e.target;
  if (img.tagName !== 'IMG' || !img.closest('.cover')) return;
  const [siguiente, ...resto] = (img.dataset.alt || '').split(' ').filter(Boolean);
  if (siguiente) { img.dataset.alt = resto.join(' '); img.src = siguiente; } else img.remove();
}, true);

// ---------------------------------------------------------------- tarjetas
function badges(r, p) {
  const b = [];
  if (p) b.push(p.tipo === 'exacto' ? '<span class="badge ok">Tu versión vale</span>'
    : p.tipo === 'posible' ? '<span class="badge warn">Sin comprobar</span>' : '<span class="badge bad">Pide otra versión</span>');
  if (A_MANO.has(r.s)) b.push('<span class="badge">Instalación manual</span>');
  else if (directa(r)) b.push('<span class="badge acc">Un clic</span>');
  if (esIA(r)) b.push('<span class="badge warn">IA</span>');
  return `<div class="badges">${b.join('')}</div>`;
}

function boton(r, p) {
  const g = p && p.g;
  if (hecha(r, g)) return `<button class="btn primary" data-act="jugar">Jugar</button>`;
  if (!p || p.tipo === 'otra' || A_MANO.has(r.s)) return `<button class="btn" data-act="ficha">Ver ficha</button>`;
  return directa(r) ? `<button class="btn primary" data-act="traducir">Traducir</button>` : `<button class="btn" data-act="ficha">Conseguir parche</button>`;
}

const versionDe = r => r.a[F.version] ? 'v' + r.a[F.version].replace(/^v\s*/i, '') : '';
let tarjetas = [];
function tarjeta({gr, r, p}) {
  const n = tarjetas.push({gr, r, p}) - 1;
  const varias = gr.filas.length > 1, fv = favorito(gr), ftxt = fv ? 'Quitar de Favoritos' : 'Añadir a Favoritos';
  return `<article class="card" data-n="${n}">
    <button type="button" class="fav" aria-pressed="${fv}" aria-label="${ftxt}" title="${ftxt}">${ICONO_FAV}</button>
    ${portada(r, hecha(r, p && p.g))}
    <div class="body">
      <button type="button" class="title" data-act="ficha"><span>${esc(gr.titulo)}</span></button>
      <div class="meta">${varias ? `<b>${gr.filas.length} traducciones</b> · ` : ''}${esc(r.a[F.grupo] || 'Grupo desconocido')}${versionDe(r) ? ' · ' + esc(versionDe(r)) : ''}</div>
      ${badges(r, p)}
      <div class="actions">${boton(r, p)}</div>
    </div>
  </article>`;
}

// ---------------------------------------------------------------- vistas
function ir(v) {
  vista = v; limit = 60;
  try { localStorage.setItem('vista', v); } catch (e) { }
  pintar();
  main.scrollTop = 0;
  // El foco pasa al contenido nuevo: con el teclado, el siguiente Tab empieza por la sección, no por el menú.
  main.focus({preventScroll: true});
}

// Qué tenía el foco antes de repintar, para devolvérselo después: cada repintado cambia todo el HTML y, si
// no, el teclado vuelve al principio de la ventana con cada pulsación.
function recordarFoco(raiz) {
  const a = document.activeElement;
  if (!a || a === document.body || !raiz.contains(a)) return null;
  if (a.id) return {id: a.id};
  const card = a.closest('.card');
  if (card) return {n: card.dataset.n, sel: a.classList.contains('fav') ? '.fav' : a.dataset.act ? `[data-act="${a.dataset.act}"]` : '.title'};
  for (const k of ['f', 'g', 'go', 'lista', 'l', 'act', 'portada', 'tanda']) if (a.dataset[k] !== undefined) return {sel: `[data-${k}="${CSS.escape(a.dataset[k])}"]`};
  return null;
}
function devolverFoco(raiz, f) {
  if (!f) return;
  const el = f.id ? document.getElementById(f.id) : f.n !== undefined ? raiz.querySelector(`.card[data-n="${f.n}"] ${f.sel}`) : raiz.querySelector(f.sel);
  if (el) el.focus({preventScroll: true});
}

function contadores() {
  const mios = new Set(pares.filter(p => p.tipo !== 'otra' && visible(p.r.gr)).map(p => p.r.gr)).size;
  $('n-mios').textContent = mios || '';
  $('n-hechas').textContent = gruposDe('hechas').length || '';
  const activos = trabajos.filter(t => !['hecho', 'error'].includes(t.estado)).length;
  $('n-dl').textContent = activos || '';
  return mios;
}

const marcarNav = () => document.querySelectorAll('.nav[data-go]').forEach(b => b.toggleAttribute('aria-current', b.dataset.go === vista) && b.setAttribute('aria-current', 'page'));

function pintar() {
  // Una lista que ya no existe (borrada, o recordada de la última vez): se vuelve a Inicio.
  if (vista.startsWith('lista:') && !listaDe(vista)) vista = 'inicio';
  marcarNav();
  contadores();
  tarjetas = [];
  const foco = recordarFoco(main);
  if (vista === 'inicio') pintarInicio();
  else if (vista === 'descargas') pintarDescargas();
  else if (vista === 'ajustes') pintarAjustes();
  else pintarRejilla();
  devolverFoco(main, foco);
}

function pintarInicio() {
  const mios = contadores();
  const hechas = itemsDe('hechas');
  const listas = itemsDe('mios').filter(({r, p}) => p && p.tipo !== 'otra' && !hecha(r, p.g) && !A_MANO.has(r.s))
    .sort((x, y) => directa(y.r) - directa(x.r));
  const recientes = filas.filter(r => visible(r.gr) && /^\d{4}-\d{2}-\d{2}/.test(r.a[F.fecha])).sort((x, y) => y.a[F.fecha].localeCompare(x.a[F.fecha]));
  const nuevas = [...new Set(recientes.map(r => r.gr))].slice(0, 18).map(gr => elegida(gr));
  const catalogo = gruposDe('todas');
  const fila = (titulo, items, v) => items.length ? `<div class="row-h"><h2>${titulo}</h2>${v ? `<button class="more" data-go="${v}">Ver todo</button>` : ''}</div>
    <div class="row">${items.slice(0, 18).map(tarjeta).join('')}</div>` : '';
  main.innerHTML = `
    <section class="hero">
      <h1>Tus juegos, en español</h1>
      <p>La biblioteca busca tus ROMs en tus carpetas de juegos y tus juegos de PC en Steam, GOG y Epic, te dice cuáles tienen traducción y aplica el parche. Nunca descarga juegos: solo parches para tu propia copia.</p>
      <div class="stats">
        <div class="stat" data-go="mios"><b>${mios}</b><span>de tus juegos tienen traducción</span></div>
        <div class="stat" data-go="hechas"><b>${hechas.length}</b><span>ya traducidos</span></div>
        <div class="stat" data-go="todas"><b>${catalogo.length.toLocaleString('es')}</b><span>juegos traducidos en el catálogo (${catalogo.reduce((n, gr) => n + gr.filas.length, 0).toLocaleString('es')} traducciones)</span></div>
      </div>
    </section>
    ${fila('Listas para tus juegos', listas, 'mios')}
    ${fila('Tus favoritos', itemsDe('lista:fav'), 'lista:fav')}
    ${fila('Ya traducidos', hechas, 'hechas')}
    ${fila('Novedades del catálogo', nuevas, 'todas')}
    ${!juegos.length && !escaneando ? `<p class="empty">No hay juegos tuyos todavía. Pon tus ROMs en <code>${esc((ajustes.carpetas || [])[0] || 'C:\\Juegos')}</code> (mejor en una subcarpeta por sistema, como <code>snes</code> o <code>psx</code>) y pulsa <b>Buscar mis juegos</b>.</p>` : ''}`;
}

// ---------------------------------------------------------------- recomendaciones
// Juegos al azar para Descubrir: con traducción completa y portada, sin juegos de adultos ni de itch.io, y
// de PC solo los que se venden en Steam. La tanda cambia cada día (y con «Otra tanda»), pero no con cada
// repintado. Un juego por sistema mientras se pueda, para que no salgan 18 de Super Nintendo.
let tanda = 0;
function mulberry32(a) {
  return () => { a = a + 0x6D2B79F5 | 0; let t = Math.imul(a ^ a >>> 15, 1 | a); t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 4294967296; };
}
function recomendados(n = 18) {
  const vale = r => completa(r) && !esAdulto(r) && !esItch(r) && (r.s !== 'pc' || !!r.x.steam);
  const porSis = new Map();
  for (const gr of gruposDe('todas')) {
    if (gr.a18 || !conPortada(gr.portada) || gr.filas.some(esItch) || !gr.filas.some(vale)) continue;
    anyadir(porSis, gr.s, gr);
  }
  const rnd = mulberry32(Math.floor(Date.now() / 864e5) * 7919 + tanda * 104729);
  const baraja = a => { for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(rnd() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; } return a; };
  const sistemas = baraja([...porSis.keys()]);
  for (const s of sistemas) baraja(porSis.get(s));
  const out = [];
  for (let ronda = 0; out.length < n && sistemas.some(s => porSis.get(s).length > ronda); ronda++) {
    for (const s of sistemas) { const gr = porSis.get(s)[ronda]; if (gr && out.length < n) out.push(gr); }
  }
  return out.map(gr => elegida(gr, vale)).filter(Boolean);
}
const bloqueRecomendados = () => `<div class="row-h"><h2>Recomendaciones</h2><button class="more" data-tanda>Otra tanda</button></div>
    <p class="lead">Una tanda al azar de juegos con traducción completa, distinta cada día. Busca o filtra para ver todo el catálogo.</p>
    <div class="row">${recomendados().map(tarjeta).join('')}</div>`;

function pintarRejilla() {
  const sistemas = [...new Set(gruposDe(vista).map(gr => gr.s))].sort((x, y) => (SYS[x] || x).localeCompare(SYS[y] || y));
  // Un sistema elegido en otra sección que aquí no existe dejaría la rejilla vacía con el desplegable en «Todos».
  if (filtro.sys && !sistemas.includes(filtro.sys)) filtro.sys = '';
  lista = filtrar(gruposDe(vista));
  const nTrad = lista.reduce((n, it) => n + it.gr.filas.length, 0);
  // En Descubrir, sin buscar ni filtrar nada: una tanda de juegos al azar para empezar por algún sitio.
  const recomendando = vista === 'todas' && !filtro.sys && !tokens($('q').value).length && !filtro.directa && !filtro.completa && !filtro.sinia && !filtro.steam;
  const chip = (k, t, ayuda) => `<button class="chip" data-f="${k}" aria-pressed="${filtro[k]}"${ayuda ? ` title="${ayuda}"` : ''}>${t}</button>`;
  const l = listaDe(vista);
  let vacio = 'No hay resultados con estos filtros.';
  if (vista === 'mios' && !juegos.length) vacio = escaneando ? 'Buscando tus juegos…' : `No hay juegos tuyos todavía. Pon tus ROMs en <code>${esc((ajustes.carpetas || [])[0] || 'C:\\Juegos')}</code> y pulsa <b>Buscar mis juegos</b>.`;
  else if (vista === 'mios') vacio = 'Ninguno de tus juegos tiene traducción en el catálogo con estos filtros.';
  else if (vista === 'hechas') vacio = 'Todavía no has traducido ningún juego.';
  else if (l && !l.juegos.length) vacio = l.id === 'fav' ? 'Todavía no tienes favoritos. Pulsa el corazón de la portada de un juego para guardarlo aquí.'
    : 'Esta lista está vacía. Abre la ficha de un juego y pulsa <b>Guardar en una lista</b>.';
  // Un juego guardado puede faltar si una actualización del catálogo lo ha cambiado de nombre o quitado.
  const faltan = l ? l.juegos.length - gruposDe(vista).length : 0;
  const cabecera = l ? `<div class="lista-h"><h1>${esc(l.nombre)}</h1>${l.id === 'fav' ? ''
    : '<button class="btn ghost" data-lista="renombrar">Cambiar el nombre</button><button class="btn ghost" data-lista="borrar">Borrar la lista</button>'}</div>`
    + (faltan > 0 ? `<p class="lead">${faltan === 1 ? 'Un juego de esta lista ya no está' : faltan + ' juegos de esta lista ya no están'} en el catálogo.</p>` : '')
    : `<h1>${VISTAS[vista]}</h1>`;
  main.innerHTML = `${cabecera}${recomendando ? bloqueRecomendados() : ''}
    <div class="filters">
      <select id="sys" aria-label="Sistema"><option value="">Todos los sistemas</option>${sistemas.map(s => `<option value="${esc(s)}" ${s === filtro.sys ? 'selected' : ''}>${esc(SYS[s] || s)}</option>`).join('')}</select>
      ${chip('directa', 'Traducción de un clic')}${chip('completa', 'Completas')}${chip('sinia', 'Sin IA')}${chip('steam', 'En Steam', 'Solo los juegos que se venden en Steam: fuera los gratuitos de itch.io y los que ya no están a la venta')}
      <span class="count">${lista.length.toLocaleString('es')} ${lista.length === 1 ? 'juego' : 'juegos'} · ${nTrad.toLocaleString('es')} ${nTrad === 1 ? 'traducción' : 'traducciones'}</span>
    </div>
    ${lista.length ? `<div class="grid">${lista.slice(0, limit).map(tarjeta).join('')}</div><div id="more"></div>` : `<div class="empty">${vacio}</div>`}`;
  $('sys').onchange = e => { filtro.sys = e.target.value; limit = 60; pintar(); };
  if ($('more')) masAlFinal.observe($('more'));
}

const masAlFinal = new IntersectionObserver(es => {
  if (!es.some(e => e.isIntersecting) || limit >= lista.length) return;
  const desde = limit;
  limit += 60;
  const g = main.querySelector('.grid');
  if (g) g.insertAdjacentHTML('beforeend', lista.slice(desde, limit).map(tarjeta).join(''));
}, {root: main, rootMargin: '800px'});

const ETQ = {'en-cola': 'En espera', descargando: 'Descargando', parcheando: 'Aplicando', hecho: 'Traducido', error: 'No se pudo'};
function pintarDescargas() {
  const lineas = trabajos.slice().reverse().map(t => {
    const r = porClave.get(t.clave);
    const fin = t.estado === 'hecho' && t.final;
    return `<div class="job ${esc(t.estado)}"><span class="dot"></span>
      <div><div class="t">${esc(r ? r.a[F.juego] : 'Traducción')}</div><div class="m">${esc(ETQ[t.estado] || t.estado)}: ${esc(t.mensaje || '')}</div></div>
      <div class="acts">${fin ? `<button class="btn primary" data-jugar="${esc(t.final)}" data-sis="${esc((t.clave || '').split('|')[0])}">Jugar</button><button class="btn" data-ruta="${esc(t.final)}">Ver en la carpeta</button>` : ''}</div>
    </div>`;
  }).join('');
  const r = espera && porClave.get(espera.clave);
  main.innerHTML = `<h1>Descargas</h1><p class="lead">Lo que has traducido en las últimas 12 horas.</p>
    ${espera ? `<div class="banner"><span>Esperando el parche de <b>${esc(r ? r.a[F.juego] : '')}</b> en <code>${esc(espera.dir)}</code>. Descárgalo de la página que se ha abierto y se aplicará solo.</span><button class="btn" id="cancelar-espera">Cancelar</button></div>` : ''}
    <div class="jobs">${lineas || '<div class="empty">Nada por ahora. Elige un juego en <b>Mis juegos</b> y pulsa <b>Traducir</b>.</div>'}</div>`;
  if ($('cancelar-espera')) $('cancelar-espera').onclick = () => api('cancelar_espera').then(sondear).catch(e => aviso(e.message, 'bad'));
}

// Ajustes: se editan en una copia y se guardan todos juntos.
let borrador = null;
function pintarAjustes() {
  if (!borrador) borrador = {carpetas: [...(ajustes.carpetas || [])], salida: ajustes.salida || 'junto', retrobat: ajustes.retrobat || '', tiendas: ajustes.tiendas !== false, estadisticas: ajustes.estadisticas !== false};
  const b = borrador, aparte = b.salida !== 'junto';
  main.innerHTML = `<h1>Ajustes</h1>
    <div class="set">
      <section><h2>Tus carpetas de juegos</h2>
        <p>Aquí busca la biblioteca tus ROMs y discos, también en sus subcarpetas. Si los tienes en una subcarpeta por sistema (<code>snes</code>, <code>psx</code>, <code>megadrive</code>…), los empareja mejor.</p>
        <div class="dirs">${b.carpetas.map((c, i) => `<div class="dir"><code>${esc(c)}</code><button class="btn ghost" data-abrir-dir="${i}">Abrir</button><button class="btn ghost" data-quitar="${i}">Quitar</button></div>`).join('') || '<p class="muted">Ninguna.</p>'}</div>
        <div class="line"><button class="btn" id="anyadir">Añadir carpeta…</button></div>
      </section>
      <section><h2>Dónde dejar los juegos traducidos</h2>
        <label class="opt"><input type="radio" name="salida" value="junto" ${aparte ? '' : 'checked'}> Junto al juego original, con la etiqueta [T-Es] en el nombre</label>
        <label class="opt"><input type="radio" name="salida" value="aparte" ${aparte ? 'checked' : ''}> En una carpeta aparte, en una subcarpeta por sistema</label>
        <div class="line" ${aparte ? '' : 'hidden'}><input type="text" id="salida" aria-label="Carpeta de los traducidos" value="${esc(aparte ? b.salida : '')}" placeholder="C:\\Juegos\\Traducidos"><button class="btn" id="elegir-salida">Elegir…</button></div>
      </section>
      <section><h2>Juegos de PC</h2>
        <label class="opt"><input type="checkbox" id="tiendas" ${b.tiendas ? 'checked' : ''}> Buscar los juegos instalados con Steam, GOG y Epic</label>
        <p>Las traducciones de PC se instalan a mano sobre la carpeta del juego; la biblioteca te dice cuáles tienes y te lleva a su página y a la carpeta del juego.</p>
      </section>
      <section><h2>RetroBat (opcional)</h2>
        <p>Si usas RetroBat, el botón <b>Jugar</b> abre el juego con su lanzador y el emulador que tengas elegido. Si no, se abre con el programa que Windows tenga asociado.</p>
        <div class="line"><input type="text" id="retrobat" aria-label="Carpeta de RetroBat" value="${esc(b.retrobat)}" placeholder="Sin RetroBat"><button class="btn" id="elegir-rb">Elegir…</button></div>
      </section>
      <section><h2>Estadísticas de uso</h2>
        <label class="opt"><input type="checkbox" id="estadisticas" ${b.estadisticas ? 'checked' : ''}> Contar que uso el programa, de forma anónima</label>
        <p>Como mucho una vez al día, el programa avisa a las estadísticas de la web (<a href="https://www.goatcounter.com/">GoatCounter</a>) de que se ha abierto y con qué versión. No manda tus juegos, tus carpetas ni nada que te identifique, y no usa cookies. Sirve para saber cuánta gente lo usa.</p>
      </section>
      <div class="line"><button class="btn primary" id="guardar">Guardar y buscar mis juegos</button></div>
      <section><h2>Parches que bajas tú</h2>
        <p>Cuando una traducción no tiene descarga de un clic, la biblioteca vigila tu carpeta <b>Descargas</b> y aplica el parche en cuanto termina de bajar.</p>
      </section>
      <section><h2>Juegos para adultos</h2>
        <label class="opt"><input type="checkbox" id="adultos" ${verAdultos ? 'checked' : ''}> Mostrar los juegos para adultos (+18)</label>
        <p>Sus traducciones van ocultas de serie en todas las secciones y no entran en las recomendaciones. Actívalo solo si eres mayor de edad; se guarda al momento.</p>
      </section>
      <section><h2>Acerca de</h2>
        <p>Biblioteca de traducciones ${esc(ajustes.version || '')}. Nunca descarga juegos: solo parches de traducción para tu propia copia. Guarda sus datos (trabajos y parches descargados) en tu carpeta de usuario.</p>
        <p>El catálogo se pone al día solo al abrir el programa y cada seis horas; las versiones nuevas del programa se avisan abajo a la izquierda. Si no quieres esperar:</p>
        <div class="line"><button class="btn" id="buscar-act">Buscar actualizaciones</button><span class="muted" id="buscar-act-res" aria-live="polite"></span></div>
        <p>Es gratuita y de código abierto. Las traducciones son de sus autores.</p>
        <div class="line"><a class="btn" href="${WEB}">La web</a><a class="btn" href="https://github.com/alucardog-dotcom/biblioteca-traducciones">El proyecto en GitHub</a><button class="btn" data-ruta="${esc(ajustes.datos || '')}">Abrir la carpeta de datos</button></div>
      </section>
    </div>`;
  const elegir = async titulo => api('elegir_carpeta', {titulo}).catch(e => { aviso(e.message, 'bad'); return null; });
  $('buscar-act').onclick = buscarActualizaciones;
  $('adultos').onchange = e => { guardarAdultos(e.target.checked); contadores(); aviso(e.target.checked ? 'Los juegos para adultos ya se muestran.' : 'Los juegos para adultos quedan ocultos.', 'ok'); };
  $('anyadir').onclick = async () => {
    const c = await elegir('Elige una carpeta con tus juegos');
    if (c && !b.carpetas.includes(c)) { b.carpetas.push(c); pintarAjustes(); }
  };
  main.querySelectorAll('[data-quitar]').forEach(x => x.onclick = () => { b.carpetas.splice(+x.dataset.quitar, 1); pintarAjustes(); });
  main.querySelectorAll('[data-abrir-dir]').forEach(x => x.onclick = () => verCarpeta(b.carpetas[+x.dataset.abrirDir]));
  main.querySelectorAll('input[name="salida"]').forEach(x => x.onchange = () => { b.salida = x.value === 'junto' ? 'junto' : ($('salida').value || ''); pintarAjustes(); });
  $('salida').oninput = e => { b.salida = e.target.value; };
  $('elegir-salida').onclick = async () => { const c = await elegir('Elige dónde dejar los juegos traducidos'); if (c) { b.salida = c; pintarAjustes(); } };
  $('tiendas').onchange = e => { b.tiendas = e.target.checked; };
  $('estadisticas').onchange = e => { b.estadisticas = e.target.checked; };
  $('retrobat').oninput = e => { b.retrobat = e.target.value; };
  $('elegir-rb').onclick = async () => { const c = await elegir('Elige la carpeta de RetroBat'); if (c) { b.retrobat = c; pintarAjustes(); } };
  $('guardar').onclick = async () => {
    if (b.salida !== 'junto' && !b.salida.trim()) return aviso('Elige la carpeta para los traducidos.', 'bad');
    try {
      ajustes = await api('guardar_ajustes', {carpetas: b.carpetas, salida: b.salida, retrobat: b.retrobat.trim() || null, tiendas: b.tiendas, estadisticas: b.estadisticas});
      borrador = null;
      aviso('Guardado. Buscando tus juegos…', 'ok');
      escaneando = true; pintarEscaneo(''); sondear();
      pintarAjustes();
    } catch (e) { aviso(e.message, 'bad'); }
  };
}

// ---------------------------------------------------------------- bienvenida
// La primera vez (sin ajustes guardados): qué hace el programa y dónde están tus juegos. No se crea ni
// se guarda nada hasta que pulsas Empezar; con «Ahora no» vuelve a salir la próxima vez.
function bienvenida() {
  const d = $('bienvenida'), elegidas = [];
  let tiendas = true;
  const pinta = () => {
    const sug = (ajustes.sugeridas || []).filter(s => !elegidas.includes(s.ruta));
    $('b-body').innerHTML = `<img class="logo" src="icono.svg" alt="" width="52" height="52">
      <h2 id="b-title">Te damos la bienvenida a la Biblioteca de traducciones</h2>
      <p>Busca tus juegos, te dice cuáles tienen traducción al español y la aplica. Nunca descarga juegos: solo los parches de traducción para tu propia copia, que queda intacta.</p>
      <h3>¿Dónde tienes tus juegos?</h3>
      <p>Elige las carpetas con tus ROMs y discos; también se miran sus subcarpetas. Si los tienes en una subcarpeta por sistema (<code>snes</code>, <code>psx</code>, <code>megadrive</code>…), los reconoce mejor.</p>
      <div class="dirs">${elegidas.map((c, i) => `<div class="dir"><code>${esc(c)}</code><button type="button" class="btn ghost" data-b-quitar="${i}">Quitar</button></div>`).join('')}</div>
      <div class="line">${sug.map(s => `<button type="button" class="btn" data-b-usar="${esc(s.ruta)}">Usar ${esc(s.ruta)}${s.existe ? '' : ' (se creará)'}</button>`).join('')}
        <button type="button" class="btn" id="b-elegir">Elegir otra carpeta…</button></div>
      <h3>Juegos de PC</h3>
      <label class="opt"><input type="checkbox" id="b-tiendas" ${tiendas ? 'checked' : ''}> Buscar también los juegos que tengo instalados con Steam, GOG y Epic</label>
      ${ajustes.retrobat ? `<p>Tienes RetroBat en <code>${esc(ajustes.retrobat)}</code>: el botón Jugar abrirá tus juegos con él.</p>` : ''}
      <div class="f-actions"><button type="button" class="btn primary" id="b-empezar" ${elegidas.length ? '' : 'disabled'}>Empezar</button>
        <button type="button" class="btn ghost" id="b-luego">Ahora no</button></div>
      <p>Para saber cuánta gente lo usa, el programa avisa una vez al día, de forma anónima y sin ningún dato tuyo, de que se ha abierto. Todo esto se puede cambiar después en Ajustes.</p>`;
  };
  $('b-body').onclick = async e => {
    const t = e.target.closest('button');
    if (!t) return;
    if (t.dataset.bUsar) elegidas.push(t.dataset.bUsar);
    else if (t.dataset.bQuitar) elegidas.splice(+t.dataset.bQuitar, 1);
    else if (t.id === 'b-elegir') {
      const c = await api('elegir_carpeta', {titulo: 'Elige una carpeta con tus juegos'}).catch(() => null);
      if (c && !elegidas.includes(c)) elegidas.push(c);
    } else if (t.id === 'b-luego') return d.close();
    else if (t.id === 'b-empezar') {
      try {
        ajustes = await api('guardar_ajustes', {carpetas: elegidas, salida: 'junto', retrobat: ajustes.retrobat || null, tiendas});
        d.close();
        aviso('Buscando tus juegos…', 'ok');
        escaneando = true; pintarEscaneo(''); sondear();
      } catch (err) { aviso(err.message, 'bad'); }
      return;
    } else return;
    pinta();
  };
  $('b-body').onchange = e => { if (e.target.id === 'b-tiendas') tiendas = e.target.checked; };
  pinta();
  d.showModal();
}

// ---------------------------------------------------------------- ficha
let fichaActual = null;
function abrirFicha(r, p) {
  const gr = r.gr, todas = ordenadas(gr);
  fichaActual = {gr, r, p, todas};
  const a = r.a, opciones = paresPorFila.get(r.key) || [];
  const elegido = p || opciones.find(o => o.tipo !== 'otra') || opciones[0] || null;
  $('f-title').textContent = gr.titulo;
  $('f-sub').textContent = (SYS[r.s] || r.s) + (todas.length > 1 ? ` · ${todas.length} traducciones` : a[F.grupo] ? ' · traducción de ' + a[F.grupo] : '');
  pintarGuardar(false);
  const dl =pares => '<dl class="f-grid">' + pares.filter(x => x[1]).map(x => `<dt>${x[0]}</dt><dd>${x[1]}</dd>`).join('') + '</dl>';
  const d0 = (r.x.d || [])[0];
  // Con varias traducciones, primero se elige cuál; debajo va la ficha de la elegida.
  let html = todas.length > 1 ? '<div class="f-trads">' + todas.map((t, i) => {
    const tp = mejorPar(t);
    const extra = [versionDe(t), t.a[F.estado], esIA(t) ? 'IA' : '', hecha(t, tp && tp.g) ? 'aplicada' : tp && tp.tipo === 'exacto' ? 'vale para tu copia' : '']
      .filter(Boolean).join(' · ');
    return `<button type="button" class="f-trad" data-fila="${i}" aria-pressed="${t === r}"><b>${esc(t.a[F.grupo] || 'Grupo desconocido')}</b>${extra ? `<span>${esc(extra)}</span>` : ''}</button>`;
  }).join('') + '</div>' : '';
  const buscarImg = 'https://www.google.com/search?tbm=isch&q=' + encodeURIComponent(gr.titulo + ' ' + (SYS[r.s] || r.s) + ' cover');
  html += `<div class="f-layout"><div class="f-cover">${portada(r, hecha(r, elegido && elegido.g))}
    <div class="f-portada">
      <button type="button" class="btn" data-portada="elegir">Elegir portada…</button>
      <button type="button" class="btn" data-portada="enlace">Pegar un enlace…</button>
      <div class="f-enlace" hidden><input type="text" id="f-url" aria-label="Dirección de la imagen" placeholder="https://…/portada.jpg" spellcheck="false"><button type="button" class="btn" data-portada="usar">Usar</button></div>
      ${propias[gr.k] ? '<button type="button" class="btn ghost" data-portada="quitar">Volver a la automática</button>' : ''}
      <a class="btn ghost" href="${esc(buscarImg)}" target="_blank" rel="noopener">Buscar en Google Imágenes</a>
      <p class="hint">También puedes copiar una imagen en el navegador y pulsar <kbd>Ctrl</kbd>+<kbd>V</kbd> aquí, o arrastrarla desde una carpeta.</p>
    </div></div><div>`
    + (todas.length > 1 ? `<h3 class="f-cual">Traducción de ${esc(a[F.grupo] || 'grupo desconocido')}${a[F.juego] !== gr.titulo ? ` <span class="muted">(${esc(a[F.juego])})</span>` : ''}</h3>` : '')
    + dl([['Título original', esc(a[F.orig])], ['Versión', esc(a[F.version])], ['Fecha', esc(a[F.fecha])], ['Estado', esc(a[F.estado])],
          ['Formato', esc(a[F.fmt].toUpperCase())], ['Juego que necesitas', esc(a[F.rom]).split(' | ').join('<br>')],
          ['CRC32', a[F.crc] ? `<code>${esc(a[F.crc].toUpperCase())}</code>` : '']])
    + (a[F.notas] ? `<p class="f-note">${esc(a[F.notas])}</p>` : '');

  html += '<div class="f-sec">Tu juego</div>';
  if (A_MANO.has(r.s)) {
    html += MODERNAS.has(r.s)
      ? '<p class="muted">Las traducciones de las consolas de hoy son mods: se copian en la carpeta de mods del emulador (Ryujinx, RPCS3, shadPS4, Cemu, Xenia…) o de la consola con firmware modificado, siguiendo las instrucciones del traductor. Comprueba que tu juego tiene la versión o la actualización que pide.</p>'
      : '<p class="muted">Las traducciones de PC se instalan sobre la carpeta del juego siguiendo las instrucciones del traductor (normalmente, copiar sus ficheros o ejecutar su instalador). Haz antes una copia de lo que vaya a sustituir.</p>';
    if (opciones.length) {
      html += '<div class="f-games">' + opciones.map(o => `<div class="f-pc"><span>${esc(o.g.n)} ${o.g.k && TIENDA[o.g.k] ? `<span class="badge">${TIENDA[o.g.k]}</span>` : ''} <span class="badge ${o.tipo === 'exacto' ? 'ok' : 'warn'}">${o.tipo === 'exacto' ? 'es este juego' : 'por el nombre'}</span></span>
        <button class="btn" data-carpeta-juego="${esc(o.g.f)}">Abrir la carpeta del juego</button></div>`).join('') + '</div>';
    } else if (r.s === 'pc') {
      html += '<p class="muted">No lo encuentro entre tus juegos de Steam, GOG o Epic.</p>';
    } else if (MODERNAS.has(r.s)) {
      html += '<p class="muted">No lo encuentro en tus carpetas de juegos.</p>';
    }
    // Los que están en Steam se venden en las tiendas de claves: si no lo tienes, el enlace de afiliado.
    const enSteam = (r.gr ? r.gr.filas : [r]).find(f => f.x.steam);
    if (enSteam && !opciones.length) {
      const nombre = enSteam.a[F.juego].replace(/\s*[(\[][^)\]]*[)\]]/g, '').trim();
      html += `<div class="f-compra"><a class="btn" href="${esc(INSTANT_GAMING + encodeURIComponent(nombre) + '&igr=' + AFILIADO)}" target="_blank" rel="noopener">¿No lo tienes? Búscalo en Instant Gaming</a>
        <p class="hint">Enlace de afiliado: si lo compras, me llevo una pequeña comisión sin que te cueste más.</p></div>`;
    }
    // Los que no están en Steam: la página oficial del juego, si la conocemos (no es un enlace de afiliado).
    const conPagina = (r.gr ? r.gr.filas : [r]).find(f => webSegura(f.x.pj));
    if (conPagina && !enSteam && !opciones.length) {
      html += `<div class="f-compra"><a class="btn" href="${esc(conPagina.x.pj)}" target="_blank" rel="noopener">¿No lo tienes? Página del juego en ${esc(sitioPagina(conPagina.x.pj))}${conPagina.x.pj18 ? ' (18+)' : ''}</a></div>`;
    }
  } else if (!opciones.length) {
    html += `<p class="muted">No tienes este juego. Pon tu copia en una de tus carpetas de juegos (${esc((ajustes.carpetas || []).join(', '))}) y pulsa <b>Buscar mis juegos</b>.</p>`;
  } else {
    html += '<div class="f-games">' + opciones.map((o, i) => `<label><input type="radio" name="juego" value="${i}" ${o === elegido ? 'checked' : ''} ${o.tipo === 'otra' ? 'disabled' : ''}>
      <span>${esc(o.g.n)} <span class="badge ${o.tipo === 'exacto' ? 'ok' : o.tipo === 'posible' ? 'warn' : 'bad'}">${o.tipo === 'exacto' ? 'versión correcta' : o.tipo === 'posible' ? 'versión sin comprobar' : 'es otra versión'}</span></span></label>`).join('') + '</div>';
    if (opciones.every(o => o.tipo === 'otra')) {
      html += `<p class="f-note warn">Tu copia no es la versión que pide esta traducción. Necesitas: ${esc(a[F.rom] || 'CRC32 ' + a[F.crc].toUpperCase())}.</p>`;
    }
  }

  html += '<div class="f-sec">Descarga</div>';
  if (A_MANO.has(r.s)) {
    html += a[F.url]
      ? '<p>Descárgala desde la página de la traducción y sigue sus instrucciones.</p>'
      : '<p>De esta traducción no se conoce la página: búscala en Google.</p>';
  } else if (d0) {
    html += `<p>Se descarga directamente de ${esc(FUENTE[d0.t] || d0.t)}.</p>`;
    // Las versiones se comparan sin la «v» delante: tokens() se come «v1» y «v1.0» quedaría como «0».
    const verTok = v => tokens(String(v).replace(/^v\s*/i, '')).join();
    if (d0.t === 'rhdn2021' && d0.v && a[F.version] && verTok(d0.v) !== verTok(a[F.version])) {
      html += `<p class="f-note warn">Esa copia es de 2021 y trae la versión ${esc(d0.v)}; la más reciente es la ${esc(a[F.version])}. Si la quieres, usa «Abrir página y esperar la descarga».</p>`;
    }
    if (d0.t === 'ra' && d0.v && a[F.version] && verTok(d0.v) !== verTok(a[F.version])) {
      html += `<p class="f-note warn">RetroAchievements guarda la versión ${esc(d0.v)}, no la ${esc(a[F.version])} de esta ficha. Si quieres la de la ficha, usa «Abrir página y esperar la descarga».</p>`;
    }
    const otras = [...new Set((r.x.d || []).slice(1).map(d => FUENTE[d.t] || d.t))];
    if (otras.length) html += `<p class="muted">Si esa no vale para tu copia, se prueba después con ${esc(otras.join(' y '))}.</p>`;
  } else {
    html += a[F.url]
      ? '<p>No hay descarga directa. Abre la página, descarga el parche y la biblioteca lo recogerá de tu carpeta Descargas y lo aplicará sola.</p>'
      : '<p>De esta traducción no se conoce la página, así que se busca en Google. Si encuentras el parche, descárgalo y la biblioteca lo recogerá de tu carpeta Descargas y lo aplicará sola.</p>';
  }
  const puede = elegido && elegido.tipo !== 'otra' && !A_MANO.has(r.s);
  const salida = salidaDe(r, elegido && elegido.g);
  html += '<div class="f-actions">';
  if (salida) html += '<button class="btn primary" data-act="jugar">Jugar</button><button class="btn" data-act="carpeta">Ver en la carpeta</button>';
  if (puede && d0) html += `<button class="btn${salida ? '' : ' primary'}" data-act="traducir">${salida ? 'Volver a traducir' : 'Traducir'}</button>`;
  if (puede) html += `<button class="btn" data-act="esperar">${a[F.url] ? 'Abrir página' : 'Buscar en Google'} y esperar la descarga</button>`;
  if (webSegura(a[F.url])) html += `<a class="btn" href="${esc(a[F.url])}" target="_blank" rel="noopener">Abrir página</a>`;
  // Sin página, el botón de esperar ya busca en Google.
  if (a[F.url] || !puede) html += `<a class="btn" href="https://www.google.com/search?q=${encodeURIComponent('traducción español ' + (a[F.orig] || a[F.juego]) + ' ' + (SYS[r.s] || r.s) + ' ' + a[F.grupo])}" target="_blank" rel="noopener">Buscar en Google</a>`;
  html += '</div></div></div>';
  $('f-body').innerHTML = html;
  if (!$('ficha').open) { listasCambiadas = false; $('ficha').showModal(); }
}

// Arriba de la ficha: Favoritos y el menú para guardar el juego en tus listas (o en una nueva).
function pintarGuardar(abierto) {
  const {gr} = fichaActual, fv = favorito(gr);
  const otras = misListas.filter(l => l.id !== 'fav' && enLista(l, gr)).length;
  const foco = recordarFoco($('f-guardar'));
  $('f-guardar').innerHTML = `<button type="button" class="btn" data-g="fav" aria-pressed="${fv}">${ICONO_FAV}${fv ? 'En Favoritos' : 'Favoritos'}</button>
    <button type="button" class="btn" data-g="menu" aria-expanded="${abierto}">${ICONO_LISTA}${otras ? `En ${otras} ${otras === 1 ? 'lista' : 'listas'}` : 'Guardar en una lista'}</button>
    <div class="menu-listas" ${abierto ? '' : 'hidden'}>
      <div class="m-h">Guardar en…</div>
      ${misListas.map(l => `<label class="opt"><input type="checkbox" data-l="${esc(l.id)}" ${enLista(l, gr) ? 'checked' : ''}><span>${esc(l.nombre)}</span></label>`).join('')}
      <div class="m-nueva"><input type="text" id="m-nombre" aria-label="Nombre de la lista nueva" placeholder="Lista nueva…" maxlength="80" spellcheck="false"><button type="button" class="btn" data-g="crear">Crear</button></div>
    </div>`;
  devolverFoco($('f-guardar'), foco);
}
function guardarFicha(que, l) {
  const {gr, r} = fichaActual;
  if (que === 'fav') {
    aviso(alternar(listaDe('lista:fav'), gr, r) ? 'Añadido a Favoritos.' : 'Quitado de Favoritos.', 'ok');
    return pintarGuardar(false);
  }
  if (que === 'menu') {
    pintarGuardar($('f-guardar').querySelector('.menu-listas').hidden);
    if (!$('f-guardar').querySelector('.menu-listas').hidden) $('m-nombre').focus();
    return;
  }
  if (que === 'crear') {
    l = nuevaLista($('m-nombre').value);
    if (!l) return $('m-nombre').focus();
    if (!enLista(l, gr)) alternar(l, gr, r); else guardarListas();
    aviso(`Guardado en «${l.nombre}».`, 'ok');
    return pintarGuardar(true);
  }
  if (que === 'lista' && l) {
    aviso(alternar(l, gr, r) ? `Guardado en «${l.nombre}».` : `Quitado de «${l.nombre}».`, 'ok');
    pintarGuardar(true);
  }
}

async function portadaFicha(que, gr) {
  if (que === 'elegir') {
    const origen = await api('elegir_imagen').catch(e => { aviso(e.message, 'bad'); return null; });
    if (origen) ponerPortada(gr, 'poner_portada', {origen});
  } else if (que === 'enlace') {
    const fila = $('f-body').querySelector('.f-enlace');
    fila.hidden = !fila.hidden;
    if (!fila.hidden) $('f-url').focus();
  } else if (que === 'usar') {
    const url = $('f-url').value.trim().replace(/ /g, '%20');
    if (!/^https?:\/\/\S+$/i.test(url)) return aviso('Pega el enlace de una imagen (empieza por https://).', 'bad');
    ponerPortada(gr, 'poner_portada', {origen: url});
  } else if (que === 'quitar') ponerPortada(gr, 'quitar_portada', {});
}

function juegoElegido() {
  const {r, p} = fichaActual, opciones = paresPorFila.get(r.key) || [];
  const radio = document.querySelector('#f-body input[name="juego"]:checked');
  return radio ? opciones[+radio.value] : p;
}

// ---------------------------------------------------------------- acciones
async function traducir(r, p) {
  try {
    await api('traducir', {clave: r.key, juego: p.g.f});
    aviso('Traduciendo ' + r.a[F.juego] + '…');
    sondear();
  } catch (e) { aviso(e.message, 'bad'); }
}

async function esperar(r, p) {
  const a = r.a;
  const url = a[F.url] || 'https://www.google.com/search?q=' + encodeURIComponent('traducción español ' + (a[F.orig] || a[F.juego]) + ' ' + (SYS[r.s] || r.s));
  try {
    await api('abrir_web', {url});
    await api('esperar', {clave: r.key, juego: p.g.f});
    sondear();
    ir('descargas');
  } catch (e) { aviso(e.message, 'bad'); }
}

const verCarpeta = ruta => api('ver_carpeta', {ruta}).catch(e => aviso(e.message, 'bad'));
const jugar = (ruta, sistema) => api('jugar', {ruta, sistema}).then(j => {
  // Si se ha renombrado (un .bin de Game Gear pasa a .gg), la ficha apunta ya al nombre nuevo.
  if (j.ruta !== ruta) {
    for (const k in traducidos) if (traducidos[k] === ruta) traducidos[k] = j.ruta;
    for (const [k, v] of tes) if (v === ruta) tes.set(k, j.ruta);
  }
  aviso(`Abriendo el juego con ${j.con}…`, 'ok');
}).catch(e => aviso(e.message, 'bad'));

function accion(act, r, p) {
  if (act === 'ficha') return abrirFicha(r, p);
  if (act === 'traducir') return traducir(r, p);
  if (act === 'esperar') return esperar(r, p);
  if (act === 'carpeta') return verCarpeta(salidaDe(r, p && p.g));
  if (act === 'jugar') return jugar(salidaDe(r, p && p.g), r.s);
}

main.addEventListener('click', e => {
  const go = e.target.closest('[data-go]');
  if (go) return ir(go.dataset.go);
  const f = e.target.closest('.chip[data-f]');
  if (f) { filtro[f.dataset.f] = !filtro[f.dataset.f]; limit = 60; return pintar(); }
  if (e.target.closest('[data-tanda]')) { tanda++; return pintar(); }
  if (e.target.dataset.jugar) return jugar(e.target.dataset.jugar, e.target.dataset.sis || '');
  if (e.target.dataset.ruta) return verCarpeta(e.target.dataset.ruta);
  if (e.target.dataset.lista) return accionLista(e.target.dataset.lista);
  const card = e.target.closest('.card');
  if (!card) return;
  if (e.target.closest('.fav')) return alternarFav(tarjetas[+card.dataset.n]);
  const {r, p} = tarjetas[+card.dataset.n];   // r es la traducción que enseña la tarjeta; r.gr, su juego
  const btn = e.target.closest('button[data-act]');
  if (btn) accion(btn.dataset.act, r, p);
  else abrirFicha(r, p);
});
$('f-body').addEventListener('click', e => {
  if (e.target.dataset.carpetaJuego) return verCarpeta(e.target.dataset.carpetaJuego);
  const pt = e.target.closest('[data-portada]');
  if (pt && fichaActual) return portadaFicha(pt.dataset.portada, fichaActual.gr);
  const otra = e.target.closest('.f-trad');
  if (otra && fichaActual) { const t = fichaActual.todas[+otra.dataset.fila]; return abrirFicha(t, mejorPar(t)); }
  const btn = e.target.closest('button[data-act]');
  if (!btn || !fichaActual) return;
  const p = juegoElegido(), act = btn.dataset.act;
  if (act !== 'carpeta' && act !== 'jugar' && !p) return;
  if (act !== 'carpeta') $('ficha').close();
  accion(act, fichaActual.r, p);
});
$('f-body').addEventListener('keydown', e => {
  if (e.target.id === 'f-url' && e.key === 'Enter' && fichaActual) portadaFicha('usar', fichaActual.gr);
});
$('f-guardar').addEventListener('click', e => {
  // El menú se vuelve a pintar con cada clic: que el clic no llegue a la ficha, que lo tomaría por uno de fuera.
  e.stopPropagation();
  const b = e.target.closest('button[data-g]');
  if (b && fichaActual) guardarFicha(b.dataset.g);
});
$('f-guardar').addEventListener('change', e => {
  if (e.target.dataset.l && fichaActual) guardarFicha('lista', misListas.find(l => l.id === e.target.dataset.l));
});
$('f-guardar').addEventListener('keydown', e => {
  if (e.target.id === 'm-nombre' && e.key === 'Enter' && fichaActual) guardarFicha('crear');
  // Escape con el menú de listas abierto cierra el menú, no toda la ficha.
  const menu = $('f-guardar').querySelector('.menu-listas');
  if (e.key === 'Escape' && menu && !menu.hidden && fichaActual) {
    e.preventDefault();
    e.stopPropagation();
    pintarGuardar(false);
    $('f-guardar').querySelector('[data-g="menu"]').focus();
  }
});
$('f-close').addEventListener('click', () => $('ficha').close());
// Se cierra al pulsar el fondo, pero no si solo se ha soltado ahí el ratón tras seleccionar texto dentro.
let pulsadoEnFondo = false;
$('ficha').addEventListener('pointerdown', e => { pulsadoEnFondo = e.target.id === 'ficha'; });
$('ficha').addEventListener('click', e => {
  if (e.target.id === 'ficha') { if (pulsadoEnFondo) e.target.close(); return; }
  // El menú de las listas se cierra al pulsar fuera de él.
  const menu = $('f-guardar').querySelector('.menu-listas');
  if (menu && !menu.hidden && !e.target.closest('#f-guardar')) pintarGuardar(false);
});
// Si has cambiado tus listas desde la ficha, al cerrarla las tarjetas lo reflejan.
$('ficha').addEventListener('close', () => {
  if (!listasCambiadas) return;
  listasCambiadas = false;
  if (vista === 'inicio' || listaDe(vista)) pintar(); else refrescarFavs();
});
function refrescarFavs() {
  main.querySelectorAll('.card').forEach(card => {
    const t = tarjetas[+card.dataset.n];
    if (t) marcarFav(card.querySelector('.fav'), favorito(t.gr));
  });
}
$('nueva-lista').addEventListener('click', crearLista);
$('listas-nav').addEventListener('click', e => { const b = e.target.closest('[data-go]'); if (b) ir(b.dataset.go); });
// Los enlaces se abren en el navegador, no dentro de la ventana.
document.addEventListener('click', e => {
  const a = e.target.closest('a[href^="http"]');
  if (!a) return;
  e.preventDefault();
  api('abrir_web', {url: a.href}).catch(err => aviso(err.message, 'bad'));
});

document.querySelectorAll('.nav[data-go]').forEach(b => b.addEventListener('click', () => ir(b.dataset.go)));
let tq;
$('q').addEventListener('input', () => {
  clearTimeout(tq);
  tq = setTimeout(() => { if (!['mios', 'todas', 'hechas'].includes(vista) && !listaDe(vista)) vista = 'todas'; limit = 60; pintar(); }, 150);
});
$('scan').addEventListener('click', async () => {
  try { await api('escanear'); escaneando = true; pintarEscaneo(''); sondear(); } catch (e) { aviso(e.message, 'bad'); }
});

const ventana = T.window.getCurrentWindow();

// Zoom del texto con Ctrl y la rueda, Ctrl+«+» y Ctrl+«-» (lo hace el propio WebView2: zoomHotkeysEnabled).
// Se recuerda entre sesiones: el zoom cambia devicePixelRatio, así que se mide respecto al del arranque.
function recordarZoom() {
  const base = window.devicePixelRatio;
  let guardado = 1;
  try { guardado = Math.min(3, Math.max(0.5, parseFloat(localStorage.getItem('zoom')) || 1)); } catch (e) { }
  const webview = T.webview && T.webview.getCurrentWebview();
  if (!webview) return;
  if (guardado !== 1) webview.setZoom(guardado).catch(() => {});
  window.addEventListener('resize', () => {
    const z = Math.round(window.devicePixelRatio / base * 100) / 100;
    try { localStorage.setItem('zoom', z); } catch (e) { }
  });
}
$('w-min').onclick = () => ventana.minimize();
$('w-max').onclick = async () => {
  await ventana.toggleMaximize();
  // El nombre del botón dice lo que hará ahora.
  const max = await ventana.isMaximized().catch(() => null);
  if (max !== null) $('w-max').setAttribute('aria-label', max ? 'Restaurar' : 'Maximizar');
};
$('w-close').onclick = () => ventana.close();

// ---------------------------------------------------------------- estado (trabajos, búsqueda, espera)
function pintarEscaneo(prog) {
  $('scan').disabled = escaneando;
  let t = '';
  if (escaneando) t = prog ? prog + '…' : 'Buscando tus juegos…';
  else if (juegos.length) {
    const pc = juegos.filter(g => g.s === 'pc').length, roms = juegos.length - pc;
    t = `${roms.toLocaleString('es')} ${roms === 1 ? 'ROM' : 'ROMs'} y ${pc.toLocaleString('es')} ${pc === 1 ? 'juego' : 'juegos'} de PC`;
  }
  $('scan-text').textContent = t;
}

let sondeo = null, sondeando = false, sondearOtraVez = false;
async function sondear() {
  // Una pasada cada vez: dos a la vez podrían aplicar las respuestas en orden inverso. Si llega otra
  // petición mientras tanto (un trabajo nuevo), se repite nada más terminar.
  if (sondeando) { sondearOtraVez = true; return; }
  sondeando = true;
  clearTimeout(sondeo);
  let activo = false;
  try {
    const st = await api('estado');
    const antes = new Map(trabajos.map(t => [t.id, t.estado]));
    trabajos = st.trabajos || [];
    let cambio = false, hechoNuevo = false;
    for (const t of trabajos) {
      if (antes.get(t.id) !== t.estado) cambio = true;
      if (t.estado === 'hecho' && antes.get(t.id) && antes.get(t.id) !== 'hecho') {
        traducidos[t.clave] = t.final;
        hechoNuevo = true;
        const r = porClave.get(t.clave);
        aviso((r ? r.a[F.juego] : 'El juego') + ' ya está en español.', 'ok');
      }
      if (t.estado === 'error' && antes.get(t.id) && antes.get(t.id) !== 'error') aviso(t.mensaje || 'No se pudo traducir.', 'bad');
      if (!['hecho', 'error'].includes(t.estado)) activo = true;
    }
    const eraEscaneo = escaneando;
    escaneando = !!(st.escaneo && st.escaneo.activo);
    pintarEscaneo(st.escaneo && st.escaneo.progreso);
    if (eraEscaneo && !escaneando) await cargarJuegos();
    const habiaEspera = !!espera;
    espera = st.espera;
    activo = activo || escaneando || !!espera;
    // Un juego recién traducido cambia su tarjeta («Jugar»): se repinta la rejilla, salvo con la ficha abierta.
    if (hechoNuevo && !['descargas', 'ajustes'].includes(vista) && !$('ficha').open) pintar();
    else if (cambio || habiaEspera !== !!espera) { if (vista === 'descargas') pintar(); else contadores(); }
  } catch (e) {
    $('scan-text').textContent = e.message;
  }
  sondeando = false;
  sondeo = setTimeout(sondear, sondearOtraVez ? 0 : activo ? 1500 : 10000);
  sondearOtraVez = false;
}

async function cargarJuegos() {
  const mj = await api('mis_juegos');
  juegos = mj.juegos || [];
  tes = new Map();
  for (const t of mj.tes || []) { const [s, n, p] = t.split('|'); tes.set(s + '|' + n, p); }
  emparejar();
  pintarEscaneo('');
  if (vista !== 'ajustes' && vista !== 'descargas') pintar(); else contadores();
}

async function cargarTodo() {
  cargarDatos(await api('datos'));
  propias = await api('portadas_propias').catch(() => ({}));
  const ml = await api('mis_listas').catch(() => null);
  if (ml && ml.listas) misListas = ml.listas;
  pintarListas();
  await cargarJuegos();
  pintar();
}

// Al acabar una búsqueda se recarga la lista aunque la ventana no supiera que había empezado.
T.event.listen('juegos', async () => {
  escaneando = false;
  try { await cargarJuegos(); } catch (e) { aviso(e.message, 'bad'); }
  sondear();
});
T.event.listen('trabajo', () => sondear());
// Ha llegado un catálogo más nuevo de GitHub: se vuelve a cargar sin cerrar nada. Devuelve el texto del aviso.
async function recargarCatalogo() {
  const antes = filas.length;
  cargarDatos(await api('datos'));
  emparejar();
  if (!$('ficha').open && vista !== 'ajustes') pintar(); else contadores();
  const mas = filas.length - antes;
  return `Catálogo actualizado${mas > 0 ? `: ${mas} ${mas === 1 ? 'traducción nueva' : 'traducciones nuevas'}` : ''}.`;
}
T.event.listen('catalogo', async () => {
  try { aviso(await recargarCatalogo(), 'ok'); } catch (e) { aviso(e.message, 'bad'); }
});

// Hay una versión más nueva en GitHub: sale un botón abajo a la izquierda para ir a descargarla.
function versionNueva(v) {
  if (!v || !v.url) return false;
  const b = $('nueva');
  b.textContent = `Versión ${v.version} disponible: descárgala`;
  b.onclick = () => api('abrir_web', {url: v.url}).catch(e => aviso(e.message, 'bad'));
  b.hidden = false;
  return true;
}

// El botón «Buscar actualizaciones» de Ajustes: catálogo y versión del programa, y un solo aviso con el resultado.
async function buscarActualizaciones() {
  const b = $('buscar-act'), res = $('buscar-act-res');
  b.disabled = true;
  b.textContent = 'Buscando…';
  res.textContent = '';
  let texto, tipo = 'ok';
  try {
    const r = await api('buscar_actualizaciones');
    if (r.catalogo === 'nuevo') texto = await recargarCatalogo();
    else if (r.catalogo && r.catalogo.error) { texto = `No se ha podido comprobar el catálogo: ${r.catalogo.error}`; tipo = 'bad'; }
    else texto = 'El catálogo está al día.';
    if (versionNueva(r.version)) texto += ` Hay una versión nueva del programa, la ${r.version.version}: descárgala con el botón de abajo a la izquierda.`;
    else if (tipo === 'ok') texto = r.catalogo === 'nuevo' ? `${texto} El programa está en su última versión.` : 'Todo al día: tienes el último catálogo y la última versión del programa.';
  } catch (e) { texto = e.message; tipo = 'bad'; }
  aviso(texto, tipo);
  res.textContent = texto;
  b.disabled = false;
  b.textContent = 'Buscar actualizaciones';
}

(async function inicio() {
  try { vista = localStorage.getItem('vista') || 'inicio'; } catch (e) { }
  if (!VISTAS[vista] && !vista.startsWith('lista:')) vista = 'inicio';
  ajustes = await api('ajustes').catch(() => ({}));
  try {
    await cargarTodo();
  } catch (e) {
    vista = 'ajustes';
    pintar();
    aviso(e.message, 'bad');
  }
  if (ajustes.nuevo) bienvenida();
  sondear();
  recordarZoom();
  // ¿Hay una versión más nueva en GitHub? Sale un botón abajo a la izquierda para ir a descargarla.
  setTimeout(async () => versionNueva(await api('version_nueva').catch(() => null)), 4000);
})();
