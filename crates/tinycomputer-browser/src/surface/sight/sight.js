// Reads a rendered page the way a person looks at it: what is drawn, what is
// on top, the words shown on or beside each control, and which boxes take
// text. Returns the page as an ordered list of controls and text, each with
// the containers a person would see it in. See `mod.rs` for the reply's shape.
//
// Called as `(root, limits) => reply`: `root` is a CSS selector to read
// under, or null for the whole page.
((root, limits) => {
  const base = root ? document.querySelector(root) : document.body;
  if (!base) return { ok: false, reason: 'root not found' };
  const width = window.innerWidth;
  const height = window.innerHeight;
  const squash = (text) => (text || '').replace(/\s+/g, ' ').trim();
  const clip = (text, most) => {
    const squashed = squash(text);
    return squashed.length > most ? squashed.slice(0, most - 1) + '…' : squashed;
  };
  const styles = new Map();
  const style = (element) => {
    if (!styles.has(element)) styles.set(element, getComputedStyle(element));
    return styles.get(element);
  };
  const boxes = new Map();
  const box = (element) => {
    if (!boxes.has(element)) boxes.set(element, element.getBoundingClientRect());
    return boxes.get(element);
  };
  const shown = (element) => {
    const rect = box(element);
    if (rect.width < 2 || rect.height < 2) return false;
    if (element.checkVisibility) {
      return element.checkVisibility({ opacityProperty: true, visibilityProperty: true });
    }
    const computed = style(element);
    return computed.display !== 'none' && computed.visibility !== 'hidden' && computed.opacity !== '0';
  };
  const TEXT_TYPES = ['text', 'search', 'email', 'tel', 'url', 'number', 'password'];
  const TEXT_ROLES = ['textbox', 'searchbox', 'combobox', 'spinbutton'];
  const ROLES = [
    'button', 'link', 'checkbox', 'radio', 'switch', 'tab', 'menuitem', 'menuitemcheckbox',
    'menuitemradio', 'option', 'treeitem', 'slider', 'gridcell',
  ];
  const tag = (element) => element.tagName.toLowerCase();
  const role = (element) => (element.getAttribute('role') || '').toLowerCase().split(' ')[0];
  const disabled = (element) =>
    element.disabled === true || element.getAttribute('aria-disabled') === 'true';

  const insideText = (element) => {
    for (let parent = element; parent; parent = parent.parentElement) {
      if (tag(parent) === 'textarea' || parent.isContentEditable) return true;
    }
    return false;
  };

  // A box that takes typed text: a text-like input, a text area, or the
  // outermost editable region. Whatever role the page gives it.
  const takesText = (element) => {
    const name = tag(element);
    if (name === 'textarea') return !element.readOnly;
    if (name === 'input') {
      const type = (element.getAttribute('type') || 'text').toLowerCase();
      return TEXT_TYPES.includes(type) && !element.readOnly;
    }
    return element.isContentEditable
      && !(element.parentElement && element.parentElement.isContentEditable);
  };

  // The hidden checkbox or radio a label stands in for: pages draw their own
  // box and hide the real one, and the label is what a person clicks.
  const standIn = (element) => {
    if (tag(element) !== 'label') return null;
    const input = element.control;
    if (!input || tag(input) !== 'input' || !['checkbox', 'radio'].includes(input.type)) return null;
    return shown(input) ? null : input;
  };

  const pointer = (element) => style(element).cursor === 'pointer';

  // What a person would take the element for, or null when it is not
  // something they would act on by itself.
  const kind = (element, insideControl) => {
    const name = tag(element);
    const claimed = role(element);
    if (takesText(element)) {
      return claimed === 'searchbox' || element.type === 'search' ? 'searchbox' : 'textbox';
    }
    if (name === 'input') {
      const type = (element.getAttribute('type') || 'text').toLowerCase();
      if (type === 'hidden') return null;
      if (type === 'checkbox') return claimed === 'switch' ? 'switch' : 'checkbox';
      if (type === 'radio') return 'radio';
      if (type === 'range') return 'slider';
      return 'button';
    }
    if (name === 'select') return 'combobox';
    if (standIn(element)) return standIn(element).type;
    if (TEXT_ROLES.includes(claimed)) {
      // A page's "text box" that holds no text box: a wrapper around the
      // real one, which is read instead, or a row or button to press.
      if (element.querySelector('input, textarea, [contenteditable=""], [contenteditable="true"]')) {
        return null;
      }
      return 'button';
    }
    if (ROLES.includes(claimed)) return claimed;
    if (name === 'a' && element.hasAttribute('href')) return 'link';
    if (name === 'button' || name === 'summary') return 'button';
    if (insideControl) return null;
    const tabindex = element.getAttribute('tabindex');
    const clickable = element.hasAttribute('onclick')
      || (tabindex !== null && tabindex !== '-1')
      || (pointer(element) && !(element.parentElement && pointer(element.parentElement)));
    return clickable ? 'button' : null;
  };

  // Text of the elements `ids` (space-separated) names.
  const byIds = (ids) => squash((ids || '').split(/\s+/)
    .map((id) => id && document.getElementById(id))
    .filter(Boolean)
    .map((element) => element.innerText || element.textContent)
    .join(' '));

  const ICON_WORDS = [
    'close', 'search', 'menu', 'back', 'next', 'previous', 'prev', 'forward', 'plus', 'minus',
    'add', 'remove', 'delete', 'edit', 'share', 'filter', 'sort', 'calendar', 'swap', 'cart',
    'account', 'user', 'profile', 'settings', 'home', 'help', 'info', 'play', 'pause', 'more',
    'expand', 'collapse', 'up', 'down', 'left', 'right', 'download', 'upload', 'refresh',
    'favorite', 'favourite', 'like', 'heart', 'star', 'bookmark', 'notification', 'bell',
    'logout', 'login', 'copy', 'print', 'mail', 'phone', 'location', 'map', 'clear', 'cancel',
  ];
  // A picture-only control's meaning, from the words in its own or its
  // icon's class, id, or test id: all a person would see is the picture.
  const iconWords = (element) => {
    const sources = [element, ...element.querySelectorAll('svg, i, img, span')].slice(0, 6);
    const words = new Set();
    for (const source of sources) {
      const text = [
        typeof source.className === 'string' ? source.className
          : (source.className && source.className.baseVal) || '',
        source.id || '',
        source.getAttribute('data-testid') || '',
        source.getAttribute('data-icon') || '',
      ].join(' ').toLowerCase();
      for (const word of text.split(/[^a-z]+/)) {
        if (ICON_WORDS.includes(word)) words.add(word);
      }
    }
    return [...words].slice(0, 3).join(' ');
  };

  // Visible words that are not controls: each a candidate label for a
  // field beside or above it.
  const words = [];
  const collectWords = () => {
    const walker = document.createTreeWalker(base, NodeFilter.SHOW_TEXT);
    let seen = null;
    for (let node = walker.nextNode(); node && words.length < limits.labels; node = walker.nextNode()) {
      const parent = node.parentElement;
      if (!parent || parent === seen || !squash(node.data)) continue;
      seen = parent;
      if (!shown(parent) || insideText(parent)) continue;
      if (parent.closest('button, a[href], select, option, [role="button"], [role="option"]')) continue;
      const text = clip(parent.innerText || node.data, 80);
      if (text) words.push({ element: parent, text, rect: box(parent) });
    }
  };

  // The words a person reads as a field's label: inside its box (a
  // floating label), to its left on the same line, or just above it; for a
  // checkbox or radio, just to its right.
  const nearby = (element, checkable) => {
    const field = box(element);
    let best = null;
    let bestGap = Infinity;
    for (const word of words) {
      const rect = word.rect;
      const across = Math.min(rect.bottom, field.bottom) - Math.max(rect.top, field.top);
      const along = Math.min(rect.right, field.right) - Math.max(rect.left, field.left);
      let gap = Infinity;
      const middleX = (rect.left + rect.right) / 2;
      const middleY = (rect.top + rect.bottom) / 2;
      if (middleX > field.left && middleX < field.right && middleY > field.top && middleY < field.bottom) {
        gap = 0;
      } else if (across > Math.min(rect.height, field.height) / 2 && rect.right <= field.left + 4) {
        gap = field.left - rect.right;
        if (gap > 200) gap = Infinity;
      } else if (along > 0 && rect.bottom <= field.top + 4) {
        gap = field.top - rect.bottom;
        gap = gap > 40 ? Infinity : gap + 1;
      } else if (checkable && across > 0 && rect.left >= field.right - 4) {
        gap = rect.left - field.right;
        if (gap > 40) gap = Infinity;
      }
      if (gap < bestGap && word.text.length <= 60) {
        best = word.text;
        bestGap = gap;
      }
    }
    return best;
  };

  // What a person reads as the element's name, and a description when the
  // page says more about it than it shows.
  const naming = (element, what) => {
    const aria = squash(element.getAttribute('aria-label'))
      || byIds(element.getAttribute('aria-labelledby'));
    const title = squash(element.getAttribute('title'));
    const input = standIn(element);
    if (['textbox', 'searchbox', 'combobox', 'slider'].includes(what) || (tag(element) === 'input' && !input)) {
      const labels = element.labels ? [...element.labels].map((label) => squash(label.innerText)).join(' ') : '';
      const checkable = ['checkbox', 'radio', 'switch'].includes(what);
      const name = squash(labels) || nearby(element, checkable) || squash(element.getAttribute('placeholder'))
        || aria || title || (tag(element) === 'input' ? squash(element.value) : '');
      return { name: clip(name, limits.name), description: aria && aria !== name ? clip(aria, limits.name) : '' };
    }
    const text = squash(element.innerText);
    if (text) {
      const description = aria && aria !== text && !text.includes(aria) ? clip(aria, limits.name) : '';
      return { name: clip(text, limits.name), description };
    }
    const pictured = [...element.querySelectorAll('img[alt], svg title')]
      .map((picture) => squash(picture.getAttribute('alt') || picture.textContent))
      .find(Boolean);
    const name = aria || title || pictured || '';
    if (name) return { name: clip(name, limits.name), description: '' };
    const icon = iconWords(element);
    return { name: icon, description: icon ? 'an icon' : '' };
  };

  const CARDS = { li: 'listitem', tr: 'row', article: 'article' };
  const CARD_ROLES = ['listitem', 'row', 'article', 'option', 'treeitem', 'gridcell'];
  const LANDMARKS = {
    header: 'banner', nav: 'navigation', main: 'main', footer: 'contentinfo',
    aside: 'complementary', form: 'form', fieldset: 'group', section: 'region',
  };
  const GROUP_ROLES = [
    'banner', 'navigation', 'main', 'contentinfo', 'complementary', 'form', 'search', 'region',
    'group', 'listbox', 'menu', 'menubar', 'tablist', 'radiogroup', 'grid', 'table', 'tree',
    'list', 'toolbar', 'tabpanel',
  ];
  const heading = (element) => {
    const found = element.querySelector('h1, h2, h3, h4, h5, h6, [role="heading"], legend');
    return found && shown(found) ? clip(found.innerText, 60) : '';
  };
  const labelOf = (element) => clip(
    element.getAttribute('aria-label') || byIds(element.getAttribute('aria-labelledby')), 60,
  );

  // An element that floats above the page: a dialog, or a fixed layer that
  // is not the page's own header.
  const layer = (element) => {
    const name = tag(element);
    const claimed = role(element);
    if (name === 'dialog' && element.open) return 'dialog';
    if (claimed === 'dialog' || claimed === 'alertdialog') return claimed;
    if (element.getAttribute('aria-modal') === 'true') return 'dialog';
    if (style(element).position !== 'fixed' || name === 'header' || name === 'nav') return null;
    const rect = box(element);
    if (rect.width * rect.height < width * height * 0.05 || !shown(element)) return null;
    if (rect.top <= 0 && rect.height < height * 0.25 && element.querySelector('nav, a[href]')) return null;
    return rect.width * rect.height >= width * height * 0.3 ? 'dialog' : 'popover';
  };

  const containers = new Map();
  // The container label a person would see `element` as, or null.
  const container = (element) => {
    if (containers.has(element)) return containers.get(element);
    let label = null;
    const name = tag(element);
    const claimed = role(element);
    const floating = layer(element);
    if (floating) {
      const named = labelOf(element) || heading(element);
      label = named ? `${floating} ${JSON.stringify(named)}` : floating;
    } else {
      const card = CARD_ROLES.includes(claimed) ? claimed : (!claimed && CARDS[name]);
      if (card) {
        const parent = element.parentElement;
        let ordinal = 1;
        for (let sibling = element.previousElementSibling; sibling; sibling = sibling.previousElementSibling) {
          if (sibling.tagName === element.tagName && role(sibling) === claimed) ordinal += 1;
        }
        const named = labelOf(element);
        label = named ? `${card} ${JSON.stringify(named)} #${ordinal}` : `${card} #${ordinal}`;
        if (!parent) label = null;
      } else {
        const group = GROUP_ROLES.includes(claimed) ? claimed
          : (LANDMARKS[name] || (['ul', 'ol'].includes(name) ? 'list' : null));
        if (group) {
          const named = labelOf(element) || (['region', 'group', 'form', 'dialog'].includes(group) ? heading(element) : '');
          if (group === 'region' && !named) label = null;
          else label = named ? `${group} ${JSON.stringify(named)}` : group;
        }
      }
    }
    containers.set(element, label);
    return label;
  };

  const pathOf = (element) => {
    const labels = [];
    for (let parent = element.parentElement; parent && parent !== document.documentElement; parent = parent.parentElement) {
      const label = container(parent);
      if (label) labels.push(label);
    }
    return labels.reverse();
  };

  // What is on top at the element's middle: itself, something inside it,
  // or something it sits in; anything else covers it.
  const covered = (element) => {
    const rect = box(element);
    const x = (rect.left + rect.right) / 2;
    const y = (rect.top + rect.bottom) / 2;
    if (x < 0 || y < 0 || x > width || y > height) return false;
    const hit = document.elementFromPoint(x, y);
    if (!hit || element.contains(hit) || hit.contains(element)) return false;
    const input = standIn(element);
    if (input && (hit === input || input.contains(hit))) return false;
    return !(element.labels && [...element.labels].some((label) => label.contains(hit)));
  };

  const offscreen = (element) => {
    const rect = box(element);
    return rect.bottom <= 0 || rect.top >= height || rect.right <= 0 || rect.left >= width;
  };

  const statesOf = (element, what) => {
    const states = [];
    const input = standIn(element) || element;
    const aria = (name) => element.getAttribute(`aria-${name}`);
    if (input.checked === true || aria('checked') === 'true' || aria('pressed') === 'true') states.push('checked');
    if (aria('expanded') === 'true' || (tag(element) === 'summary' && element.parentElement && element.parentElement.open)) {
      states.push('expanded');
    }
    if (aria('selected') === 'true' || (aria('current') && aria('current') !== 'false')) states.push('selected');
    if (input.required === true || aria('required') === 'true') states.push('required');
    if (offscreen(element)) states.push('offscreen');
    else if (covered(element)) states.push('covered');
    return states;
  };

  const valueOf = (element, what) => {
    const name = tag(element);
    if (name === 'select') {
      const chosen = element.selectedOptions && element.selectedOptions[0];
      return chosen ? squash(chosen.textContent) : '';
    }
    if (what === 'textbox' || what === 'searchbox') {
      if (name === 'input' && element.type === 'password') return '';
      return name === 'input' || name === 'textarea' ? element.value : element.innerText;
    }
    if (what === 'slider') return String(element.value);
    return '';
  };

  let next = Number(window.__tinycomputerSeen || 1);
  const mark = (element) => {
    let id = element.getAttribute('data-tc-seen');
    if (!id) {
      id = String(next);
      next += 1;
      element.setAttribute('data-tc-seen', id);
    }
    return id;
  };

  collectWords();
  const nodes = [];
  const controls = new Set();
  let unreachable = 0;
  let texts = 0;
  const insideControl = (element) => {
    for (let parent = element.parentElement; parent; parent = parent.parentElement) {
      if (controls.has(parent)) return true;
    }
    return false;
  };
  const inFront = (element) => {
    const rect = box(element);
    const x = Math.min(Math.max((rect.left + rect.right) / 2, 0), width - 1);
    const y = Math.min(Math.max((rect.top + rect.bottom) / 2, 0), height - 1);
    const hit = document.elementFromPoint(x, y);
    return hit === element || (hit && element.contains(hit));
  };

  const walker = document.createTreeWalker(base, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => {
      if (node.nodeType === Node.ELEMENT_NODE
        && ['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE', 'HEAD'].includes(node.tagName)) {
        return NodeFilter.FILTER_REJECT;
      }
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let lastText = null;
  for (let node = base; node; node = walker.nextNode()) {
    if (node.nodeType === Node.TEXT_NODE) {
      const parent = node.parentElement;
      if (!parent || !squash(node.data) || texts >= limits.texts) continue;
      if (lastText && lastText.contains(parent)) continue;
      if (insideControl(parent) || insideText(parent) || !shown(parent)) continue;
      const rect = box(parent);
      if (rect.bottom < -height || rect.top > 2 * height) continue;
      lastText = parent;
      texts += 1;
      nodes.push({ text: clip(parent.innerText || node.data, limits.text), path: pathOf(parent) });
      continue;
    }
    const element = node;
    if (element.shadowRoot && shown(element)
      && element.shadowRoot.querySelector('a[href], button, input, select, textarea, [role], [tabindex]')) {
      unreachable += 1;
    }
    if (tag(element) === 'iframe' && shown(element) && !offscreen(element) && inFront(element)) {
      const rect = box(element);
      if (rect.width * rect.height >= width * height * 0.2) unreachable += 1;
    }
    if (controls.size >= limits.controls || disabled(element)) continue;
    const what = kind(element, insideControl(element));
    if (!what || !shown(element)) continue;
    if (tag(element) === 'input' && (element.type === 'checkbox' || element.type === 'radio')) {
      // Drawn by its label instead: the label stands in for it.
      if ([...(element.labels || [])].some((label) => standIn(label) === element)) continue;
    }
    controls.add(element);
    const { name, description } = naming(element, what);
    nodes.push({
      id: mark(element),
      role: what,
      name,
      description,
      value: valueOf(element, what),
      states: statesOf(element, what),
      box: [box(element).x, box(element).y, box(element).width, box(element).height].map(Math.round),
      path: pathOf(element),
    });
  }
  window.__tinycomputerSeen = next;

  const middle = document.elementFromPoint(width / 2, height / 2);
  let surface = 'window';
  for (let parent = middle; parent; parent = parent.parentElement) {
    const floating = layer(parent);
    if (floating === 'alertdialog') { surface = 'alert'; break; }
    if (floating === 'dialog') { surface = 'sheet'; break; }
  }
  return { ok: true, surface, unreachable, nodes };
})
