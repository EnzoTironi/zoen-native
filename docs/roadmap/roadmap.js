'use strict';

(() => {
  const $ = (id) => document.getElementById(id);
  const repo = 'https://github.com/EnzoTironi/zoen-native';
  const phases = [
    ['active-ios', '1 · iOS em andamento'],
    ['later-ios', '2 · Completar o iOS'],
    ['paused-creation', '3 · Criação · pausada'],
    ['production', '4 · Produção'],
    ['web-android', '5 · Web e Android'],
    ['desktop', '6 · Desktop'],
  ];
  const labels = {
    scope: { active: 'Ativo', later: 'Depois', paused: 'Pausado', decision: 'Referência adicional' },
    implementation: {
      present: 'Código presente', partial: 'Parcial', missing: 'Ausente',
      unverified: 'Não verificada', different: 'Modelo diferente',
    },
    acceptance: { accepted: 'Aceito', scoped: 'Evidência parcial', pending: 'Aceite pendente' },
  };
  const viewCopy = {
    roadmap: ['Jornadas do produto', 'O que a pessoa precisa conseguir fazer, do início à recuperação.'],
    surfaces: ['Telas e apresentações', 'Rotas, sheets, menus e superfícies nativas. Código não substitui revisão visual.'],
    telegram: ['Referência Telegram', 'Cada recurso é uma decisão de produto, não uma promessa automática de paridade.'],
  };
  const state = {
    tab: 'roadmap', phase: 'all', search: '', scope: 'all', implementation: 'all', acceptance: 'all',
    rows: { roadmap: [], surfaces: [], telegram: [] }, manifest: null, failures: [],
  };
  const featureNames = new Map();
  const areaNames = {
    'Messaging and composition': 'Mensagens e composição',
    'Formatting and rich text': 'Formatação e texto rico',
    'Inbox, folders and Saved Messages': 'Inbox, pastas e Salvos',
    'Search, discovery and reading': 'Busca, descoberta e leitura',
    'Reactions, stickers and emoji': 'Reações, stickers e emoji',
    'Photos, video and attachments': 'Fotos, vídeos e anexos',
    'Voice, audio and video messages': 'Mensagens de voz, áudio e vídeo',
    'Calls, video chats and live streams': 'Chamadas, vídeo e transmissões ao vivo',
    'Groups, topics and communities': 'Grupos, tópicos e comunidades',
    'Channels and publishing': 'Canais e publicação',
    'Polls and checklists': 'Enquetes e checklists',
    'Stories': 'Stories',
    'Profiles and contacts': 'Perfis e contatos',
    'Privacy, security and account lifecycle': 'Privacidade, segurança e ciclo da conta',
    'Business and chat automation': 'Negócios e automação de conversas',
    'Bots and agent experiences': 'Bots e experiências com agentes',
    'Apps and embedded experiences': 'Apps e experiências incorporadas',
    'AI, translation and text assistance': 'IA, tradução e assistência de texto',
    'Premium subscriptions and benefits': 'Assinaturas Premium e benefícios',
    'Boosts and community entitlements': 'Impulsos e benefícios de comunidades',
    'Stars, commerce, gifts and creator economy': 'Stars, comércio, presentes e economia de criadores',
    'Notifications, appearance and accessibility': 'Notificações, aparência e acessibilidade',
    'Storage, connectivity and platform integration': 'Armazenamento, conectividade e integração de plataforma',
  };
  const text = (value) => typeof value === 'string' ? value : value == null ? '' : JSON.stringify(value);
  const list = (value) => Array.isArray(value) ? value : value ? [value] : [];
  const folded = (value) => text(value).normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase();
  function node(tag, className, value) {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (value != null) element.textContent = text(value);
    return element;
  }
  function link(label, href, className) {
    const element = node('a', className, label);
    element.href = href;
    element.target = '_blank';
    element.rel = 'noopener noreferrer';
    return element;
  }
  function accepted(row) {
    return row.acceptance === 'accepted' || row.accepted === true;
  }
  function acceptance(row) {
    return accepted(row) ? 'accepted' : row.acceptance === 'scoped' ? 'scoped' : 'pending';
  }
  function normalizedScope(row) {
    if (row.scope) return row.scope;
    if (row.phase === 'paused-creation') return 'paused';
    return row.phase === 'active-ios' || row.phase === 'production' ? 'active' : 'later';
  }
  function normalize(row, source, index) {
    const feature = featureNames.get(row.id);
    const entry = {
      ...row,
      id: row.id || `${source}:${index}`,
      title: row.title || row.name || row.feature || feature?.feature || row.path || row.id || 'Item sem título',
      area: row.area || feature?.area || row.category || row.kind || 'Outras superfícies',
      scope: normalizedScope(row),
      acceptance: acceptance(row),
      implementation: row.implementation || 'unverified',
      evidence: list(row.evidence),
      issues: list(row.issues),
      remaining: list(row.remaining || row.gaps || row.gap),
      source,
    };
    if (row.path && !entry.evidence.includes(row.path)) entry.evidence.unshift(row.path);
    entry.searchText = folded([
      entry.title, entry.id, entry.area, entry.path, entry.entrypoint,
      entry.remaining.join(' '), entry.recommendation, entry.issues.map((issue) => `#${issue}`).join(' '),
    ].join(' '));
    return entry;
  }
  async function json(path) {
    const response = await fetch(path);
    if (!response.ok) throw new Error(`${path} · HTTP ${response.status}`);
    return response.json();
  }
  function paths(value) {
    if (typeof value === 'string') return [value];
    if (Array.isArray(value)) return value.flatMap(paths);
    if (value && typeof value === 'object') return Object.values(value).flatMap(paths);
    return [];
  }
  async function category(files) {
    const loaded = [];
    for (const path of new Set(paths(files))) {
      try {
        const data = await json(path);
        const rows = Array.isArray(data) ? data : data.rows;
        if (!Array.isArray(rows)) throw new Error(`${path} · formato de linhas não reconhecido`);
        loaded.push(rows.map((row, index) => normalize(row, path, index)));
      } catch (error) {
        state.failures.push(error.message);
      }
    }
    const unique = new Map();
    loaded.flat().forEach((row) => {
      if (unique.has(row.id)) state.failures.push(`ID duplicado: ${row.id}`);
      else unique.set(row.id, row);
    });
    return [...unique.values()];
  }
  function evidenceHref(value) {
    if (typeof value !== 'string') return null;
    if (/^https?:\/\//i.test(value)) return value;
    if (/^[a-z][a-z0-9+.-]*:/i.test(value) || value.startsWith('/') || value.includes('..')) return null;
    const match = value.match(/^(.*?):(\d+)$/);
    const path = match ? match[1] : value;
    const commit = state.manifest?.sourceCommit;
    if (!commit || !/^[0-9a-f]{7,40}$/i.test(commit)) return null;
    return `${repo}/blob/${commit}/${path.split('/').map(encodeURIComponent).join('/')}${match ? `#L${match[2]}` : ''}`;
  }
  function badge(kind, value) {
    return node('span', `badge ${value}`, labels[kind]?.[value] || value);
  }
  function record(row) {
    const detail = node('details', 'record');
    const summary = node('summary');
    const check = node('input', 'accept-check');
    check.type = 'checkbox';
    check.checked = accepted(row);
    check.disabled = true;
    check.setAttribute('aria-label', accepted(row) ? 'Jornada aceita' : 'Jornada ainda não aceita');
    const title = node('span', 'row-title', row.title);
    const subtitle = row.phase ? phases.find(([id]) => id === row.phase)?.[1] : row.path || row.id;
    title.append(node('span', 'row-subtitle', subtitle || row.id));
    const badges = node('span', 'badges');
    badges.append(badge('scope', row.scope), badge('implementation', row.implementation), badge('acceptance', row.acceptance));
    summary.append(check, title, badges);
    detail.append(summary);
    const body = node('div', 'detail');
    if (row.entrypoint) body.append(node('p', '', row.entrypoint));
    if (row.recommendation) {
      body.append(node('h4', '', 'Recomendação'));
      list(row.recommendation).forEach((item) => body.append(node('p', '', item)));
    }
    body.append(node('h4', '', 'O que falta fechar'));
    if (row.remaining.length) {
      const items = node('ul');
      row.remaining.forEach((item) => items.append(node('li', '', item)));
      body.append(items);
    } else {
      body.append(node('p', '', accepted(row)
        ? 'Nenhuma pendência registrada neste recorte aceito.'
        : 'Nenhuma lacuna detalhada neste inventário. O aceite completo ainda precisa ser registrado.'));
    }
    if (row.visualEvidence) body.append(node('p', 'row-subtitle', `Evidência visual: ${row.visualEvidence === 'source-only' ? 'somente código; revisão visual pendente' : text(row.visualEvidence)}`));
    if (row.evidence.length) {
      body.append(node('h4', '', 'Evidência na fonte'));
      const items = node('ul', 'evidence');
      row.evidence.forEach((item) => {
        const value = typeof item === 'string' ? item : item.path || item.url;
        const href = evidenceHref(value);
        const li = node('li');
        if (href) li.append(link(value, href));
        else li.append(node('span', '', value || text(item)));
        items.append(li);
      });
      body.append(items);
    }
    if (row.issues.length) {
      body.append(node('h4', '', 'Entrega acompanhada'));
      const issues = node('div', 'issue-links');
      row.issues.forEach((issue) => {
        const number = text(issue).replace(/^#/, '');
        if (/^\d+$/.test(number)) issues.append(link(`#${number}`, `${repo}/issues/${number}`));
      });
      body.append(issues);
    }
    body.append(node('p', 'accept-note', row.acceptance === 'accepted'
      ? 'Aceite registrado na fonte. Consulte a evidência para conhecer o alcance.'
      : row.acceptance === 'scoped'
        ? 'Há evidência para um recorte. A jornada inteira continua sem aceite.'
        : 'Aceite pendente. Presença de código e aparência visual não comprovam a jornada completa.'));
    detail.append(body);
    return detail;
  }
  function phaseFor(row) {
    return phases.some(([id]) => id === row.phase) ? row.phase : 'unassigned';
  }
  function grouped(rows) {
    const groups = new Map();
    if (state.tab === 'roadmap') phases.forEach(([id]) => groups.set(id, []));
    rows.forEach((row) => {
      const key = state.tab === 'roadmap' ? phaseFor(row) : row.area;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(row);
    });
    return groups;
  }
  function render() {
    const rows = state.rows[state.tab];
    const query = folded(state.search.trim());
    const shown = rows.filter((row) =>
      (!query || row.searchText.includes(query)) &&
      (state.scope === 'all' || row.scope === state.scope) &&
      (state.implementation === 'all' || row.implementation === state.implementation) &&
      (state.acceptance === 'all' || row.acceptance === state.acceptance) &&
      (state.tab !== 'roadmap' || state.phase === 'all' || row.phase === state.phase));
    document.querySelectorAll('[data-tab]').forEach((button) => {
      button.setAttribute('aria-pressed', String(button.dataset.tab === state.tab));
    });
    $('phase-filter').hidden = state.tab !== 'roadmap';
    $('phase-filter').querySelectorAll('button').forEach((button) => {
      button.setAttribute('aria-pressed', String(button.dataset.phase === state.phase));
    });
    $('view-title').textContent = viewCopy[state.tab][0];
    $('view-description').textContent = viewCopy[state.tab][1];
    $('reset').hidden = !query && ['scope', 'implementation', 'acceptance', 'phase'].every((key) => state[key] === 'all');
    const fragment = document.createDocumentFragment();
    for (const [key, entries] of grouped(shown)) {
      if (!entries.length) continue;
      const section = node('section', 'group');
      const name = state.tab === 'roadmap'
        ? phases.find(([id]) => id === key)?.[1] || 'Etapa a definir'
        : areaNames[key] || key;
      const heading = node('h3', 'group-title', name);
      heading.append(node('span', '', entries.length));
      section.append(heading);
      entries.forEach((row) => section.append(record(row)));
      fragment.append(section);
    }
    if (!shown.length) fragment.append(node('p', 'empty', rows.length
      ? 'Nenhum item corresponde aos filtros. Experimente outra busca ou limpe os filtros.'
      : 'Nenhum dado carregado nesta seção. Consulte o aviso e o manifesto.'));
    $('content').replaceChildren(fragment);
    $('content').setAttribute('aria-busy', 'false');
    $('result-count').textContent = `${shown.length} de ${rows.length} ${state.tab === 'surfaces' ? 'registros' : 'itens'} · ${shown.filter(accepted).length} com aceite registrado neste recorte`;
  }
  function overview() {
    const counts = state.manifest?.counts || {};
    const journeys = counts.journeys ?? state.rows.roadmap.filter((row) => !row.source.includes('creation-capabilities')).length;
    const creation = counts.creationCapabilities ?? state.rows.roadmap.filter((row) => row.source.includes('creation-capabilities')).length;
    const metrics = [
      [journeys, 'jornadas no roadmap', `${creation} capacidades de criação`],
      [counts.screenFamilies ?? '—', 'famílias de telas na fonte', `${state.rows.surfaces.length} registros no inventário`],
      [counts.telegramFeatures ?? state.rows.telegram.length, 'recursos Telegram mapeados', 'referências para avaliar'],
      [state.rows.roadmap.filter(accepted).length, 'itens com aceite registrado', 'jornadas e capacidades de criação'],
    ];
    $('overview').replaceChildren(...metrics.map(([count, label, note]) => {
      const element = node('div', 'metric');
      element.append(node('strong', '', count), node('span', '', label));
      element.append(node('small', '', note));
      return element;
    }));
    Object.keys(state.rows).forEach((key) => { $(`count-${key}`).textContent = state.rows[key].length; });
  }
  function decisions() {
    const values = list(state.manifest.priorityDecisions);
    $('decisions').hidden = !values.length;
    values.forEach((item) => {
      const article = node('article', 'decision');
      const copy = node('div');
      copy.append(node('p', '', item.decision));
      if (item.reason) copy.append(node('p', 'reason', item.reason));
      article.append(node('h3', '', item.title), copy);
      $('decisions').append(article);
    });
  }
  async function visualReview() {
    if (!state.manifest.visualReview) return;
    try {
      const data = await json(state.manifest.visualReview);
      const screens = list(data.screens);
      if (!screens.length) return;
      $('visuals').hidden = false;
      $('visuals').querySelector(':scope > p').textContent = `App 718 / Core 707 · Simulator. ${screens.length} capturas de estados, não ${screens.length} telas distintas. Sem aceite funcional, aparelho físico ou paridade Telegram.`;
      for (const screen of screens) {
        const card = node('figure', 'visual-card');
        const imagePath = typeof screen.image === 'string' && !/^[a-z][a-z0-9+.-]*:/i.test(screen.image) ? screen.image : null;
        if (imagePath) {
          const anchor = link('Abrir captura', imagePath);
          const image = node('img');
          image.src = imagePath;
          image.alt = screen.name || 'Captura de tela do Zoen';
          image.loading = 'lazy';
          image.addEventListener('error', () => { anchor.replaceWith(node('p', '', 'Captura indisponível; evidência visual pendente.')); }, { once: true });
          anchor.replaceChildren(image);
          card.append(anchor);
        } else card.append(node('p', '', 'Evidência visual pendente.'));
        card.append(node('figcaption', '', screen.name || screen.path));
        list(screen.findings).forEach((finding) => card.append(node('p', '', finding)));
        if (screen.path) {
          const href = evidenceHref(screen.path);
          if (href) card.append(link('Abrir fonte ↗', href));
        }
        $('visual-grid').append(card);
      }
    } catch {
      // Optional visual evidence never blocks the roadmap itself.
      $('visuals').hidden = false;
      $('visual-grid').append(node('p', '', 'Revisão visual ainda indisponível. Os aceites permanecem independentes.'));
    }
  }
  function controls() {
    document.querySelectorAll('[data-tab]').forEach((button) => {
      button.addEventListener('click', () => { state.tab = button.dataset.tab; render(); });
    });
    for (const key of ['search', 'scope', 'implementation', 'acceptance']) {
      $(key).addEventListener(key === 'search' ? 'input' : 'change', (event) => {
        state[key] = event.target.value;
        render();
      });
    }
    for (const [id, label] of [['all', 'Todas as etapas'], ...phases]) {
      const button = node('button', '', label);
      button.type = 'button';
      button.dataset.phase = id;
      button.addEventListener('click', () => { state.phase = id; render(); });
      $('phase-filter').append(button);
    }
    $('reset').addEventListener('click', () => {
      state.search = '';
      $('search').value = '';
      for (const key of ['scope', 'implementation', 'acceptance']) state[key] = $(key).value = 'all';
      state.phase = 'all';
      render();
    });
  }
  async function start() {
    controls();
    try {
      state.manifest = await json('manifest.json');
      try {
        const original = await json('../../output/telegram-feature-matrix-2026-10-20/matrix.json');
        list(original.features || original.rows || original).forEach((row) => featureNames.set(row.id, row));
      } catch {
        state.failures.push('Nomes originais Telegram indisponíveis; os IDs continuam consultáveis.');
      }
      const files = state.manifest.files || {};
      [state.rows.roadmap, state.rows.surfaces, state.rows.telegram] = await Promise.all([
        category([...paths(files.journeys), ...paths(files.creation)]), category(files.surfaces), category(files.telegram),
      ]);
      const commit = text(state.manifest.sourceCommit);
      $('revision').textContent = `${text(state.manifest.date)} · fonte ${commit.slice(0, 8) || 'não informada'}`;
      overview();
      decisions();
      if (state.failures.length) {
        $('notice').hidden = false;
        $('notice').textContent = `Levantamento incompleto: ${state.failures.join(' · ')}. Contagens mostram apenas os itens carregados.`;
      }
      render();
      await visualReview();
    } catch (error) {
      $('revision').textContent = 'Fonte indisponível';
      $('notice').hidden = false;
      $('notice').textContent = `Não foi possível carregar o manifesto. Abra esta página por um servidor HTTP na raiz do repositório e confira manifest.json. ${error.message}`;
      overview();
      render();
    }
  }
  void start();
})();
