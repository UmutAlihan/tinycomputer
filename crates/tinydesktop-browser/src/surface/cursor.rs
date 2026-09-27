//! The agent's cursor, drawn in the page.
//!
//! Before the surface acts on an element, a second cursor glides onto it
//! along a human path (`tinydesktop-cursor`) and pulses as it lands; then the
//! engine performs the action exactly as it would with no cursor on screen.
//! The cursor is purely cosmetic: it is an inert, `aria-hidden`,
//! click-through element in a closed shadow root, so it receives no events,
//! stays out of accessibility snapshots and hit tests, and sends no input.
//! It is drawn only where someone can see it — a headed session, or one
//! attached to an existing browser.

use serde_json::{Value, json};
use tinydesktop_cursor::{Glide, Rect};

use super::BrowserSurface;

/// Mounts the cursor once per document, then animates it along the path it
/// is given — `[t_ms, x, y]` triples, the first the starting point — and
/// pulses where it lands. A new glide cancels one still running.
const CURSOR_JS: &str = r##"((path, appears) => {
  const KEY = '__tinydesktopCursor';
  let c = globalThis[KEY];
  if (!c || !c.host.isConnected) {
    const host = document.createElement('tinydesktop-cursor');
    host.setAttribute('aria-hidden', 'true');
    host.setAttribute('inert', '');
    host.style.cssText = 'all:initial!important;position:fixed!important;inset:0!important;z-index:2147483647!important;pointer-events:none!important;overflow:visible!important;contain:layout style!important;';
    const shadow = host.attachShadow({ mode: 'closed' });
    shadow.innerHTML = `<style>
      :host, * { pointer-events: none !important; }
      .cursor { position: fixed; top: 0; left: 0; opacity: 0; transition: opacity .15s; will-change: transform; }
      .cursor.shown { opacity: 1; }
      svg { display: block; width: 26px; height: 26px; overflow: visible; filter: drop-shadow(0 2px 3px #0006); }
      .pulse { position: fixed; width: 44px; height: 44px; margin: -22px; border-radius: 50%; border: 2px solid #7c5cff; background: #7c5cff33; box-sizing: border-box; animation: pulse .45s ease-out forwards; }
      @keyframes pulse { from { transform: scale(.2); opacity: .9; } to { transform: scale(1); opacity: 0; } }
    </style><div class="cursor"><svg viewBox="0 0 24 24"><path d="M1 1L1 19L6 14.5L9.5 22L12.5 20.7L9 13.4L16 13.4Z" fill="#7c5cff" stroke="white" stroke-width="1.6" stroke-linejoin="round"/></svg></div>`;
    document.documentElement.appendChild(host);
    c = globalThis[KEY] = { host, shadow, cursor: shadow.querySelector('.cursor'), frame: 0 };
    appears = true;
  }
  cancelAnimationFrame(c.frame);
  const place = (x, y) => { c.cursor.style.transform = `translate3d(${x}px,${y}px,0)`; };
  const [, x0, y0] = path[0];
  if (appears) place(x0, y0);
  c.cursor.classList.add('shown');
  const end = path[path.length - 1];
  const pulse = () => {
    const ring = document.createElement('div');
    ring.className = 'pulse';
    ring.style.left = `${end[1]}px`;
    ring.style.top = `${end[2]}px`;
    ring.addEventListener('animationend', () => ring.remove(), { once: true });
    c.shadow.insertBefore(ring, c.cursor);
  };
  const start = performance.now();
  let i = 0;
  const step = (now) => {
    const t = now - start;
    while (i < path.length - 2 && path[i + 1][0] <= t) i++;
    if (t >= end[0]) { place(end[1], end[2]); pulse(); return; }
    const [ta, xa, ya] = path[i], [tb, xb, yb] = path[i + 1];
    const k = tb > ta ? Math.min(1, Math.max(0, (t - ta) / (tb - ta))) : 1;
    place(xa + (xb - xa) * k, ya + (yb - ya) * k);
    c.frame = requestAnimationFrame(step);
  };
  c.frame = requestAnimationFrame(step);
  return true;
})"##;

/// The script call that animates `glide`: its path as rounded
/// `[t_ms, x, y]` triples, starting from where the glide begins.
pub(super) fn script(glide: &Glide) -> String {
    let round = |value: f64| (value * 10.0).round() / 10.0;
    let path: Vec<Value> = std::iter::once(json!([0.0, round(glide.from.x), round(glide.from.y)]))
        .chain(glide.samples.iter().map(|sample| {
            json!([
                round(sample.t_ms),
                round(sample.point.x),
                round(sample.point.y)
            ])
        }))
        .collect();
    format!("{CURSOR_JS}({}, {})", Value::Array(path), glide.appears)
}

impl BrowserSurface {
    /// Whether anyone can see this surface's page, and so its cursor.
    pub(super) fn shows_cursor(&self) -> bool {
        !self.cursor_pace.is_off() && (!self.options.headless || self.options.endpoint.is_some())
    }

    /// Where `reference` is on the page, in viewport pixels.
    fn bounds(&self, reference: &str) -> Option<Rect> {
        let id = self.ensure_session().ok()?;
        let selector = format!("@{}", reference.trim_start_matches('@'));
        let data = self
            .block(
                self.browser
                    .command(&id, json!({"action": "boundingbox", "selector": selector})),
            )
            .ok()?;
        let field = |name: &str| data.get(name).and_then(Value::as_f64);
        let rect = Rect::new(field("x")?, field("y")?, field("width")?, field("height")?);
        (rect.is_valid() && rect.width > 0.0 && rect.height > 0.0).then_some(rect)
    }

    /// Glides the drawn cursor onto `reference` and waits for it to land, so
    /// the action that follows is seen where it happens. Does nothing when
    /// no one can see the page, the element has no box, or the page will not
    /// run the script; the action proceeds regardless.
    pub(super) fn show_cursor(&self, reference: &str) {
        if !self.shows_cursor() {
            return;
        }
        let Some(rect) = self.bounds(reference) else {
            return;
        };
        let Some(glide) = self
            .cursor
            .lock()
            .ok()
            .and_then(|mut cursor| cursor.glide(rect))
        else {
            return;
        };
        let Ok(id) = self.ensure_session() else {
            return;
        };
        let drawn = self.block(
            self.browser
                .command(&id, json!({"action": "evaluate", "script": script(&glide)})),
        );
        if drawn.is_ok() {
            (self.wait)(std::time::Duration::from_secs_f64(
                glide.duration_ms() / 1_000.0,
            ));
        } else if let Ok(mut cursor) = self.cursor.lock() {
            cursor.forget();
        }
    }
}
