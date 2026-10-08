// Runs at document start in the isolated "muse-client" world.
// Exposes __museFill(text, submit, selector), which types `text` into the
// Muse composer, and __museFocus(selector), which focuses it. Both return true
// when a visible composer was found.
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

  window.__museFocus = (selector) => {
    const el = findComposer(selector);
    if (!el) return false;
    el.focus();
    return true;
  };

  window.__museFill = (text, submit, selector) => {
    const el = findComposer(selector);
    if (!el) return false;
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
    if (submit) {
      const opts = { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true };
      el.dispatchEvent(new KeyboardEvent('keydown', opts));
      el.dispatchEvent(new KeyboardEvent('keyup', opts));
    }
    return true;
  };
})();
