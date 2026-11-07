// Motor de parches en el navegador: IPS, BPS, UPS, PPF (1, 2 y 3) y APS (N64 y GBA), más CRC32 y lectura
// de zips. Es motor.rs (y antes PatchEngine.cs) pasado a JavaScript, con las mismas comprobaciones, y trabaja
// en memoria con Uint8Array. Los xdelta no se aplican aquí (hace falta xdelta3): se avisa.
// Vale como script en una página (window.Motor) y en Node (globalThis.Motor) para las pruebas.
(function (raiz) {
'use strict';

const MAX_EN_MEMORIA = 1800 * 1024 * 1024;
const MAX_N64 = 128 * 1024 * 1024;

// ---------------------------------------------------------------- CRC32
const TABLA = new Uint32Array(256);
for (let i = 0; i < 256; i++) {
  let c = i;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1;
  TABLA[i] = c >>> 0;
}
// Por trozos (para leer ficheros grandes sin cargarlos enteros): c = crcAvanza(c, trozo) desde CRC_INICIO, y
// crcFin(c) al acabar.
const CRC_INICIO = 0xFFFFFFFF;
function crcAvanza(c, b, desde = 0, hasta = b.length) {
  for (let i = desde; i < hasta; i++) c = TABLA[(c ^ b[i]) & 0xFF] ^ (c >>> 8);
  return c >>> 0;
}
const crcFin = c => (c ^ 0xFFFFFFFF) >>> 0;
const crc32 = (b, desde = 0, hasta = b.length) => crcFin(crcAvanza(CRC_INICIO, b, desde, hasta));
const hex = c => (c >>> 0).toString(16).toUpperCase().padStart(8, '0');

// ---------------------------------------------------------------- lectura
const truncado = () => new Error('Parche truncado o dañado.');
function u32le(b, p) {
  if (p < 0 || p + 4 > b.length) throw truncado();
  return (b[p] | b[p + 1] << 8 | b[p + 2] << 16 | b[p + 3] << 24) >>> 0;
}
const u64le = (b, p) => u32le(b, p) + u32le(b, p + 4) * 4294967296;
// Número de longitud variable de BPS y UPS. `pos` es {i} para que avance.
function varint(p, pos) {
  let dato = 0, paso = 1;
  for (;;) {
    if (pos.i >= p.length) throw truncado();
    const x = p[pos.i++];
    dato += (x & 0x7f) * paso;
    if (x & 0x80) return dato;
    if (paso > 2 ** 56) throw new Error('Parche dañado.');
    paso *= 128;
    dato += paso;
  }
}
const empieza = (b, s) => b.length >= s.length && [...s].every((ch, i) => b[i] === ch.charCodeAt(0));

function detectar(b) {
  if (empieza(b, 'PATCH')) return 'ips';
  if (empieza(b, 'BPS1')) return 'bps';
  if (empieza(b, 'UPS1')) return 'ups';
  if (empieza(b, 'PPF10')) return 'ppf1';
  if (empieza(b, 'PPF20')) return 'ppf2';
  if (empieza(b, 'PPF30')) return 'ppf3';
  if (empieza(b, 'APS10')) return 'apsn64';
  if (empieza(b, 'APS1')) return 'apsgba';
  if (b.length >= 4 && b[0] === 0xD6 && b[1] === 0xC3 && b[2] === 0xC4 && b[3] === 0) return 'xdelta';
  return null;
}

// Lo que el parche sabe del juego que pide: BPS y UPS llevan el CRC32 del original y del traducido; PPF 2 y
// 3, un bloque del disco original.
function inspeccionar(p) {
  const info = {formato: detectar(p), crcOrigen: null, crcDestino: null, ppfBloque: null, ppfTam: null};
  if (info.formato === 'bps' || info.formato === 'ups') {
    if (p.length < 16) throw new Error('Parche demasiado pequeño.');
    info.crcOrigen = u32le(p, p.length - 12);
    info.crcDestino = u32le(p, p.length - 8);
  } else if (info.formato === 'ppf2' || info.formato === 'ppf3') {
    if (p.length >= 1084) {
      if (info.formato === 'ppf2') {
        info.ppfTam = u32le(p, 56);
        info.ppfBloque = {desde: 0x9320, bloque: p.slice(60, 1084)};
      } else if (p[57] !== 0) {
        info.ppfBloque = {desde: p[56] === 0 ? 0x9320 : 0x80A0, bloque: p.slice(60, 1084)};
      }
    }
  }
  return info;
}

const iguales = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);

// ¿Es `origen` el juego que pide el parche? 'vale', 'ya' (ya está parcheado), 'no' o 'desconocido'.
function comprobar(info, origen) {
  if (info.crcOrigen !== null) {
    const c = crc32(origen);
    return c === info.crcOrigen ? 'vale' : c === info.crcDestino ? 'ya' : 'no';
  }
  if (info.ppfBloque) {
    const {desde, bloque} = info.ppfBloque;
    return desde + 1024 <= origen.length && iguales(origen.subarray(desde, desde + 1024), bloque) ? 'vale' : 'no';
  }
  return 'desconocido';
}

// ---------------------------------------------------------------- aplicar
// `origen` ya viene sin la cabecera (de copiador de SNES o iNES) si la hay que saltar: lo decide quien llama.
// Devuelve {datos, mensaje}; si el parche no vale, lanza un Error con el motivo.
function aplicar(p, origen, forzar = false) {
  if (origen.length > MAX_EN_MEMORIA) throw new Error('El juego es demasiado grande para parchearlo en el navegador.');
  switch (detectar(p)) {
    case 'ips': return ips(p, origen);
    case 'bps': return bps(p, origen, forzar);
    case 'ups': return ups(p, origen, forzar);
    case 'ppf1': case 'ppf2': case 'ppf3': return ppf(p, origen, forzar);
    case 'apsn64': return apsN64(p, origen);
    case 'apsgba': return apsGba(p, origen);
    case 'xdelta': throw new Error('Los parches xdelta no se pueden aplicar en el navegador: usa el programa.');
    default: throw new Error('Formato de parche no reconocido.');
  }
}

function reservar(tam) {
  if (tam > MAX_EN_MEMORIA) throw new Error('El resultado sería demasiado grande.');
  return new Uint8Array(tam);
}
function conTamano(datos, tam) {
  if (tam === datos.length) return datos;
  const out = reservar(tam);
  out.set(datos.subarray(0, Math.min(tam, datos.length)));
  return out;
}

function ips(p, origen) {
  const u24 = i => i + 3 <= p.length ? p[i] * 65536 + p[i + 1] * 256 + p[i + 2] : null;
  const faltaEof = () => new Error('Parche IPS truncado (falta EOF).');
  const roto = () => new Error('Parche IPS truncado.');
  // Primero se leen los bloques, para saber cuánto crece el juego; luego se escriben.
  const bloques = [];
  let pos = 5, fin = origen.length, tamFinal = null;
  for (;;) {
    const desde = u24(pos);
    if (desde === null) throw faltaEof();
    pos += 3;
    if (desde === 0x454F46) {
      // Tras «EOF» pueden venir exactamente 3 bytes: el tamaño final del juego (truncado). Cualquier otra
      // cosa sobrante (relleno, otro formato pegado) no es un tamaño y se ignora.
      if (p.length === pos + 3) tamFinal = u24(pos);
      break;
    }
    if (pos + 2 > p.length) throw roto();
    const tam = p[pos] * 256 + p[pos + 1];
    pos += 2;
    if (tam === 0) {
      if (pos + 3 > p.length) throw roto();
      const n = p[pos] * 256 + p[pos + 1], valor = p[pos + 2];
      pos += 3;
      bloques.push({desde, n, valor});
      fin = Math.max(fin, desde + n);
    } else {
      if (pos + tam > p.length) throw roto();
      bloques.push({desde, datos: p.subarray(pos, pos + tam)});
      pos += tam;
      fin = Math.max(fin, desde + tam);
    }
  }
  let out = conTamano(origen, fin);
  if (out === origen) out = origen.slice();
  for (const b of bloques) {
    if (b.datos) out.set(b.datos, b.desde);
    else out.fill(b.valor, b.desde, b.desde + b.n);
  }
  if (tamFinal !== null) out = conTamano(out, tamFinal);
  return {datos: out, mensaje: `IPS aplicado (${bloques.length} bloques). El formato IPS no permite verificar el resultado.`};
}

function comprobarCrcParche(p, formato) {
  const n = p.length - 4;
  if (crc32(p, 0, n) !== u32le(p, n)) throw new Error(`El fichero ${formato} está dañado (CRC del parche incorrecto).`);
}

// '' si el juego es el que pide el parche; una nota si se ha forzado; error si no vale.
function comprobarCrcOrigen(real, origen, destino, forzar) {
  if (real === origen) return '';
  if (real === destino) throw new Error('Este juego ya tiene aplicado este parche.');
  if (!forzar) throw new Error(`El juego no es la versión que pide el parche (CRC32 ${hex(real)}, se esperaba ${hex(origen)}).`);
  return ` (forzado: CRC de origen ${hex(real)} en vez de ${hex(origen)})`;
}

function bps(p, src, forzar) {
  if (p.length < 19) throw new Error('Parche BPS demasiado pequeño.');
  comprobarCrcParche(p, 'BPS');
  const pos = {i: 4};
  varint(p, pos);
  const tamDestino = varint(p, pos);
  const meta = varint(p, pos);
  pos.i += meta;
  const crcO = u32le(p, p.length - 12), crcD = u32le(p, p.length - 8), fin = p.length - 12;
  const nota = comprobarCrcOrigen(crc32(src), crcO, crcD, forzar);
  const tgt = reservar(tamDestino);
  const noEncaja = () => new Error('El parche BPS no encaja con este juego.');
  let sal = 0, relO = 0, relD = 0;
  while (pos.i < fin) {
    const dato = varint(p, pos);
    const n = Math.floor(dato / 4) + 1;
    if (sal + n > tgt.length) throw noEncaja();
    const modo = dato & 3;
    if (modo === 0) {
      // Copiar del original en la misma posición.
      for (let i = sal; i < sal + n; i++) tgt[i] = i < src.length ? src[i] : 0;
    } else if (modo === 1) {
      // Bytes nuevos que trae el parche.
      if (pos.i + n > p.length) throw noEncaja();
      tgt.set(p.subarray(pos.i, pos.i + n), sal);
      pos.i += n;
    } else {
      // Copiar de otra parte del original o de lo ya escrito.
      const d = varint(p, pos);
      const salto = d & 1 ? -Math.floor(d / 2) : Math.floor(d / 2);
      let rel = (modo === 2 ? relO : relD) + salto;
      if (rel < 0) throw noEncaja();
      for (let i = sal; i < sal + n; i++) {
        if (modo === 2) {
          if (rel >= src.length) throw noEncaja();
          tgt[i] = src[rel];
        } else {
          if (rel >= i) throw noEncaja();
          tgt[i] = tgt[rel];
        }
        rel++;
      }
      if (modo === 2) relO = rel; else relD = rel;
    }
    sal += n;
  }
  if (crc32(tgt) !== crcD) throw new Error('El resultado no supera la verificación CRC32; no se ha guardado.');
  return {datos: tgt, mensaje: `BPS aplicado y verificado (CRC32 ${hex(crcD)})${nota}.`};
}

function ups(p, src, forzar) {
  if (p.length < 18) throw new Error('Parche UPS demasiado pequeño.');
  comprobarCrcParche(p, 'UPS');
  const pos = {i: 4};
  varint(p, pos);
  const tamDestino = varint(p, pos);
  const crcO = u32le(p, p.length - 12), crcD = u32le(p, p.length - 8), fin = p.length - 12;
  const nota = comprobarCrcOrigen(crc32(src), crcO, crcD, forzar);
  const tgt = conTamano(src, tamDestino);
  const out = tgt === src ? src.slice() : tgt;
  let rel = 0;
  while (pos.i < fin) {
    rel += varint(p, pos);
    for (;;) {
      if (pos.i >= fin) throw new Error('Parche UPS truncado.');
      const x = p[pos.i++];
      if (rel < out.length) out[rel] = (rel < src.length ? src[rel] : 0) ^ x;
      rel++;
      if (x === 0) break;
    }
  }
  if (crc32(out) !== crcD) throw new Error('El resultado no supera la verificación CRC32; no se ha guardado.');
  return {datos: out, mensaje: `UPS aplicado y verificado (CRC32 ${hex(crcD)})${nota}.`};
}

// Dónde empieza la descripción FILE_ID.DIZ del final de un PPF (o el final, si no la lleva).
function inicioDiz(p, datos) {
  const abre = '@BEGIN_FILE_ID.DIZ', cierra = '@END_FILE_ID.DIZ';
  const en = (i, s) => i >= 0 && i + s.length <= p.length && [...s].every((ch, k) => p[i + k] === ch.charCodeAt(0));
  // La etiqueta de cierre va seguida de 2 (PPF3) o 4 (PPF2) bytes de longitud.
  const conCierre = [2, 4].some(cola => { const i = p.length - cola - cierra.length; return i >= datos && en(i, cierra); });
  if (!conCierre || p.length < abre.length) return p.length;
  const desde = Math.max(datos, p.length - 70000);
  for (let i = p.length - abre.length; i >= desde; i--) if (en(i, abre)) return i;
  return p.length;
}

function ppf(p, origen, forzar) {
  const info = inspeccionar(p);
  let nota = '';
  if (comprobar(info, origen) === 'no') {
    if (!forzar) throw new Error('El disco no es la versión que pide el parche PPF (falla la comprobación de bloque).');
    nota = ' (forzado: la comprobación de bloque fallaba)';
  }
  if (info.ppfTam && origen.length !== info.ppfTam && !forzar) throw new Error('El tamaño del disco no coincide con el que pide el parche PPF2.');
  const version = info.formato === 'ppf1' ? 1 : info.formato === 'ppf2' ? 2 : 3;
  const ppf3 = version === 3;
  const deshacer = ppf3 && p.length > 58 && p[58] !== 0;
  let pos = version === 1 ? 56 : version === 2 ? 1084 : (p.length > 57 && p[57] !== 0 ? 1084 : 60);
  const fin = inicioDiz(p, pos);
  const out = origen.slice();
  const largo = out.length;
  let bloques = 0, igualesN = 0;
  while (pos < fin) {
    let desde;
    if (ppf3) {
      if (pos + 9 > fin) break;
      desde = u64le(p, pos);
      pos += 8;
    } else {
      if (pos + 5 > fin) break;
      desde = u32le(p, pos);
      pos += 4;
    }
    const n = p[pos++];
    if (pos + n > fin) throw new Error('Parche PPF truncado.');
    const nuevo = p.subarray(pos, pos + n);
    if (desde + n <= largo && iguales(out.subarray(desde, desde + n), nuevo)) igualesN++;
    // Un PPF escribe dentro del disco (como mucho, justo al final): un desplazamiento mayor es un parche
    // corrupto o de otra imagen, no una orden de agrandar el fichero gigabytes.
    if (desde > largo) throw new Error('El parche PPF apunta fuera del disco: no es para esta imagen o está dañado.');
    if (desde + n > out.length) throw new Error('El parche PPF apunta fuera del disco: no es para esta imagen o está dañado.');
    out.set(nuevo, desde);
    pos += n;
    if (deshacer) pos += n;
    bloques++;
  }
  if (bloques === 0) throw new Error('El parche PPF no tiene ningún bloque: está truncado o dañado.');
  if (igualesN === bloques) throw new Error('Este disco ya tiene aplicado este parche.');
  const que = info.ppfBloque ? 'PPF aplicado (bloque de comprobación correcto)' : 'PPF aplicado (este PPF no incluye comprobación)';
  return {datos: out, mensaje: `${que}, ${bloques} bloques${nota}.`};
}

function apsN64(p, origen) {
  if (p.length < 61) throw new Error('Parche APS demasiado pequeño.');
  let pos = 7 + 50, tamFinal = null;
  if (p[5] === 1) {
    // Formato original, ID del cartucho, país, CRC y relleno; después, el tamaño final.
    pos += 1 + 3 + 1 + 8 + 5;
    tamFinal = u32le(p, pos);
    pos += 4;
  }
  let out = origen.slice();
  if (tamFinal) {
    // Ningún cartucho de N64 pasa de 64 MB: un tamaño final mayor es un parche dañado.
    if (tamFinal > MAX_N64) throw new Error('El parche APS pide un tamaño final imposible para un juego de N64.');
    out = conTamano(out, tamFinal);
  }
  const roto = () => new Error('Parche APS truncado.');
  const asegurar = fin => { if (fin > out.length) out = conTamano(out, fin); };
  while (pos + 5 <= p.length) {
    const desde = u32le(p, pos), n = p[pos + 4];
    pos += 5;
    if (n === 0) {
      if (pos + 2 > p.length) throw roto();
      asegurar(desde + p[pos + 1]);
      out.fill(p[pos], desde, desde + p[pos + 1]);
      pos += 2;
    } else {
      if (pos + n > p.length) throw roto();
      asegurar(desde + n);
      out.set(p.subarray(pos, pos + n), desde);
      pos += n;
    }
  }
  return {datos: out, mensaje: 'APS (N64) aplicado. Este formato no permite verificar el resultado.'};
}

function apsGba(p, src) {
  if (p.length < 12) throw new Error('Parche APS demasiado pequeño.');
  const tamOrigen = u32le(p, 4), tamDestino = u32le(p, 8);
  if (src.length !== tamOrigen) throw new Error('El tamaño del juego no coincide con el que pide el parche APS.');
  const tgt = conTamano(src, tamDestino);
  const out = tgt === src ? src.slice() : tgt;
  const BLOQUE = 0x10000;
  let pos = 12;
  while (pos + 8 + BLOQUE <= p.length) {
    // Desplazamiento y dos CRC16 del bloque.
    const desde = u32le(p, pos);
    pos += 8;
    for (let i = 0; i < BLOQUE; i++) {
      const j = desde + i;
      if (j >= out.length) break;
      out[j] = (j < src.length ? src[j] : 0) ^ p[pos + i];
    }
    pos += BLOQUE;
  }
  return {datos: out, mensaje: 'APS (GBA) aplicado.'};
}

// ---------------------------------------------------------------- zip
// Lo justo para sacar un parche de un .zip: sin ZIP64 ni cifrado. Descomprime con DecompressionStream.
const esZip = b => b.length > 4 && b[0] === 0x50 && b[1] === 0x4B && b[2] === 3 && b[3] === 4;
function zipEntradas(b) {
  let eocd = -1;
  for (let i = b.length - 22; i >= Math.max(0, b.length - 65557); i--) {
    if (b[i] === 0x50 && b[i + 1] === 0x4B && b[i + 2] === 5 && b[i + 3] === 6) { eocd = i; break; }
  }
  if (eocd < 0) throw new Error('El zip está dañado o incompleto.');
  const n = b[eocd + 10] | b[eocd + 11] << 8;
  let p = u32le(b, eocd + 16);
  const out = [], dec = new TextDecoder('utf-8');
  for (let k = 0; k < n; k++) {
    if (u32le(b, p) !== 0x02014B50) throw new Error('El zip está dañado (índice).');
    const flags = b[p + 8] | b[p + 9] << 8, metodo = b[p + 10] | b[p + 11] << 8;
    const crc = hex(u32le(b, p + 16)), comp = u32le(b, p + 20), tam = u32le(b, p + 24);
    const nlen = b[p + 28] | b[p + 29] << 8, elen = b[p + 30] | b[p + 31] << 8, clen = b[p + 32] | b[p + 33] << 8;
    const off = u32le(b, p + 42);
    const nombre = dec.decode(b.subarray(p + 46, p + 46 + nlen));
    if (!nombre.endsWith('/')) out.push({nombre, metodo, comp, tam, off, crc, cifrado: !!(flags & 1)});
    p += 46 + nlen + elen + clen;
  }
  return out;
}
async function zipExtraer(b, e) {
  if (e.cifrado) throw new Error(`El zip está cifrado (${e.nombre}).`);
  const p = e.off;
  if (u32le(b, p) !== 0x04034B50) throw new Error('El zip está dañado (entrada).');
  const nlen = b[p + 26] | b[p + 27] << 8, elen = b[p + 28] | b[p + 29] << 8;
  const ini = p + 30 + nlen + elen;
  if (ini + e.comp > b.length) throw new Error('El zip está incompleto.');
  const datos = b.subarray(ini, ini + e.comp);
  if (e.metodo === 0) return datos.slice();
  if (e.metodo !== 8) throw new Error(`El zip usa una compresión que el navegador no sabe abrir (método ${e.metodo}).`);
  const resp = new Response(new Blob([datos]).stream().pipeThrough(new DecompressionStream('deflate-raw')));
  return new Uint8Array(await resp.arrayBuffer());
}

// Los ficheros de parche que hay en un zip, por preferencia (como descarga.rs): fuera las herramientas y
// el código fuente de los traductores.
const ORDEN_PARCHE = ['bps', 'xdelta', 'ups', 'ips', 'ppf', 'aps', 'vcdiff'];
function parchesDe(entradas) {
  return entradas
    .map(e => {
      const n = e.nombre.toLowerCase(), ext = n.includes('.') ? n.slice(n.lastIndexOf('.') + 1) : '';
      const tools = [...n.matchAll(/tool/g)].some(m => !/^[a-z0-9]/.test(n.slice(m.index + 4).replace(/^s/, '')));
      if (n.includes('source') || n.includes('src') || n.includes('herramienta') || tools) return null;
      const pos = ORDEN_PARCHE.indexOf(ext);
      return pos < 0 ? null : {e, pos};
    })
    .filter(Boolean)
    .sort((a, b) => a.pos - b.pos)
    .map(x => x.e);
}

// Cabecera que no cuenta en el CRC de las bases de datos: 16 bytes en NES (iNES) y 512 en las ROMs de SNES
// con cabecera de copiador (el tamaño sobra en 512 sobre un múltiplo de 1024).
function cabecera(b) {
  if (empieza(b, 'NES\x1a')) return 16;
  if (b.length % 1024 === 512) return 512;
  return 0;
}

raiz.Motor = {crc32, CRC_INICIO, crcAvanza, crcFin, hex, detectar, inspeccionar, comprobar, aplicar, esZip, zipEntradas, zipExtraer, parchesDe, cabecera, MAX_EN_MEMORIA};
})(typeof window !== 'undefined' ? window : globalThis);
