// Runs at document start in the isolated "calliope" world.
// Exposes, for the app's prompt pre-fill:
//   __calliopeEnsure(text, selector): "hidden" while the page is off screen
//     (it may not lay out its form then), "missing" when there is no visible
//     composer, "present" when it already holds `text`, else types `text`
//     in and returns "filled". The app calls it until the text survives a
//     second, because the page may re-render and wipe an early fill.
//   __calliopeSubmit(selector): presses Enter in the composer.
//   __calliopeFocus(selector): focuses the composer; true when found.
(() => {
  const DEFAULT_SELECTORS = [
    'textarea',
    '[contenteditable="true"]',
    '[role="textbox"]',
  ];

  const isVisible = (el) => !!el && el.getClientRects().length > 0 && !el.disabled;

  const findComposer = (selector) => {
    const selectors = selector ? [selector, ...DEFAULT_SELECTORS] : DEFAULT_SELECTORS;
    for (const sel of selectors) {
      let nodes;
      try {
        nodes = document.querySelectorAll(sel);
      } catch (_e) {
        continue;
      }
      for (const el of nodes) {
        if (isVisible(el)) return el;
      }
    }
    return null;
  };

  const setNativeValue = (el, text) => {
    const proto = el instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype;
    const desc = Object.getOwnPropertyDescriptor(proto, 'value');
    if (desc && desc.set) {
      desc.set.call(el, text); // React-compatible: bypasses the instance tracker
    } else {
      el.value = text;
    }
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };

  window.__calliopeFocus = (selector) => {
    const el = findComposer(selector);
    if (!el) return false;
    el.focus();
    return true;
  };

  const normalize = (s) => s.replace(/\s+/g, ' ').trim();

  const contentOf = (el) => (el.isContentEditable ? el.innerText : el.value) || '';

  // What the composer showed right after our last fill: an editor may turn
  // typed text into its own form (smart quotes, link chips), which still
  // counts as present.
  const filled = new WeakMap();

  window.__calliopeEnsure = (text, selector) => {
    if (document.visibilityState === 'hidden') return 'hidden';
    const el = findComposer(selector);
    if (!el) return 'missing';
    const now = normalize(contentOf(el));
    if (now && (now === normalize(text) || now === filled.get(el))) return 'present';
    el.focus();
    if (el.isContentEditable) {
      document.execCommand('selectAll', false, null);
      if (!document.execCommand('insertText', false, text)) {
        el.textContent = text;
        el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
      }
    } else {
      setNativeValue(el, text);
    }
    filled.set(el, normalize(contentOf(el)));
    return 'filled';
  };

  window.__calliopeSubmit = (selector) => {
    const el = findComposer(selector);
    if (!el) return false;
    const opts = { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true };
    el.dispatchEvent(new KeyboardEvent('keydown', opts));
    el.dispatchEvent(new KeyboardEvent('keyup', opts));
    return true;
  };
})();
