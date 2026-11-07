'use strict';
// Buscador del catálogo: carga juegos.json ([título, sistema, ruta, traducciones, un clic, palabras, portada])
// y filtra mientras escribes, sin distinguir acentos ni mayúsculas. Los juegos salen como tarjetas con su
// portada, como en el programa. La búsqueda queda en el enlace (?q=).
(function () {
  const q = document.getElementById('q'), s = document.getElementById('s'), res = document.getElementById('res');
  const cuenta = document.getElementById('cuenta'), mas = document.getElementById('mas');
  const POR_VEZ = 60;
  let juegos = [], hallados = [], vistos = 0;
  const norm = t => t.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
  const esc = t => String(t).replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
  const num = n => n.toLocaleString('es-ES');
  const FAV = '<button type="button" class="fav" aria-pressed="false" aria-label="Añadir a Favoritos"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 20s-7.5-4.6-9.3-9.2C1.4 7.4 3.6 4 7 4c2 0 3.6 1.1 5 3 1.4-1.9 3-3 5-3 3.4 0 5.6 3.4 4.3 6.8C19.5 15.4 12 20 12 20z"/></svg></button>';

  // Las direcciones de la portada, en el orden en que se prueban (las mismas que hace web.py para las
  // páginas): la puesta a mano; la de Steam, la del catálogo y la cabecera de Steam; o la de libretro, la
  // del catálogo y la pantalla de título.
  function portadas(spec) {
    if (!spec) return [];
    const tipo = spec[0], resto = spec.slice(2).split('|');
    if (tipo === 'm') return ['../' + resto[0]];
    if (tipo === 'u') return [resto[0]];
    if (tipo === 's') {
      const st = 'https://cdn.cloudflare.steamstatic.com/steam/apps/' + encodeURIComponent(resto[0]) + '/';
      return [st + 'library_600x900.jpg', resto[1], st + 'header.jpg'].filter(Boolean);
    }
    const [sis, t, nombre, pu] = resto;
    const alt = t === 'Named_Boxarts' ? 'Named_Titles' : 'Named_Snaps';
    const thumb = tt => 'https://thumbnails.libretro.com/' + encodeURIComponent(sis) + '/' + tt + '/' + encodeURIComponent(nombre.replace(/[&*/:`<>?\\|"]/g, '_')) + '.png';
    return [thumb(t), pu, thumb(alt)].filter(Boolean);
  }

  function tarjeta(j) {
    const urls = portadas(j[6]);
    const img = urls.length ? `<img loading="lazy" src="${esc(urls[0])}" data-alt="${esc(urls.slice(1).join(' '))}" alt="" referrerpolicy="no-referrer" onerror="sig(this)" onload="this.classList.add('ok')">` : '';
    const meta = [esc(SYS[j[1]] || j[1]), j[3] > 1 ? `${j[3]} traducciones` : '', j[4] ? '<b>Un clic</b>' : ''].filter(Boolean).join(' · ');
    return `<li class="tarjeta" data-ruta="${esc(j[2])}" data-sis="${esc(j[1])}" data-sisn="${esc(SYS[j[1]] || j[1])}" data-titulo="${esc(j[0])}"><a href="../${j[2]}"><span class="port${img ? '' : ' sin'}">${img}<span class="ph">${esc(j[0])}</span></span>` +
      `<span class="cuerpo"><span class="t">${esc(j[0])}</span><span class="m">${meta}</span></span></a>${FAV}</li>`;
  }

  function pintar() {
    const trozo = hallados.slice(vistos, vistos + POR_VEZ);
    res.insertAdjacentHTML('beforeend', trozo.map(tarjeta).join(''));
    vistos += trozo.length;
    mas.hidden = vistos >= hallados.length;
  }

  function filtrar() {
    const palabras = norm(q.value).split(/[^a-z0-9]+/).filter(Boolean);
    hallados = juegos.filter(j => (!s.value || j[1] === s.value) && palabras.every(p => j.t.includes(p)));
    // Primero los que tienen la búsqueda en el título (no solo en otro nombre o en el grupo) y los que más traducciones tienen.
    if (palabras.length) {
      const enTitulo = j => palabras.every(p => j.n.includes(p)) ? 1 : 0;
      hallados.sort((a, b) => enTitulo(b) - enTitulo(a) || b[3] - a[3]);
    }
    vistos = 0;
    res.innerHTML = '';
    pintar();
    const n = hallados.length;
    if (!q.value && !s.value) cuenta.textContent = `${num(n)} juegos. Escribe para buscar.`;
    else if (n) cuenta.textContent = `${num(n)} ${n === 1 ? 'juego' : 'juegos'}`;
    else cuenta.textContent = 'No hay ningún juego con ese nombre. Prueba con menos palabras o con su nombre en inglés.';
    const p = new URLSearchParams();
    if (q.value) p.set('q', q.value);
    if (s.value) p.set('s', s.value);
    try { history.replaceState(null, '', p.toString() ? '?' + p : location.pathname); } catch (e) { }
  }

  const p = new URLSearchParams(location.search);
  if (p.get('q')) q.value = p.get('q');
  if (p.get('s')) s.value = p.get('s');
  q.addEventListener('input', filtrar);
  s.addEventListener('change', filtrar);
  mas.addEventListener('click', pintar);
  cuenta.textContent = 'Cargando el catálogo…';
  fetch('juegos.json')
    .then(r => r.json())
    .then(d => {
      juegos = d.map(j => Object.assign(j, {n: norm(j[0]), t: norm(j[0]) + ' ' + j[5]}));
      filtrar();
    })
    .catch(() => { cuenta.textContent = 'No se pudo cargar el catálogo. Recarga la página.'; });
})();
