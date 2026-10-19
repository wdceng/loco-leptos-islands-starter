// For htmx 4 only (htmx 2 names its events differently: htmx:afterSwap).
// Keeps Leptos islands alive in HTML htmx swaps in. One of the template's
// two hand-written scripts, with island-loader-timeout.js: plumbing, no
// page logic (CLAUDE.md, "Islands, htmx and Alpine").

// In blocks, so these consts stay out of the page's global scope.
{
  // 1. Chrome 150 to 156 drops the comments Leptos hydrates by when
  //    parseHTMLUnsafe gets one argument; with options it keeps them
  //    (https://github.com/bigskysoftware/htmx/issues/4147, the htmx
  //    maintainer's way). Remove once Chrome 157 is everywhere.
  const parse = Document.parseHTMLUnsafe?.bind(Document);
  if (parse) {
    Document.parseHTMLUnsafe = (html, options) => parse(html, { ...options });
  }
}

{
  // 2. Wake islands htmx swaps in, as Leptos's own router does.
  //    Islands already on the page are Leptos's first pass: mark them.
  for (const el of document.querySelectorAll("leptos-island")) {
    el.$$hydrated = true;
  }
  const wake = () => {
    const hydrate = window.__hydrateIsland;
    for (const el of document.querySelectorAll("leptos-island")) {
      if (el.$$hydrated) continue;
      el.$$hydrated = true;
      // A failure while the wasm still starts: the first pass takes it.
      if (hydrate) hydrate(el, el.dataset.component).catch(() => {});
    }
  };
  document.addEventListener("htmx:after:swap", wake);

  // 3. htmx 4 doesn't watch the page: process hx-* added later, and wake
  //    islands added later (the swap event may fire on a removed element).
  const HX =
    "[hx-get],[hx-post],[hx-put],[hx-patch],[hx-delete],[hx-action],[hx-trigger]";
  new MutationObserver((records) => {
    let islands = false;
    for (const record of records) {
      for (const node of record.addedNodes) {
        if (node.nodeType !== 1) continue;
        // Not where htmx was blocked: the page works as plain links then.
        if (window.htmx && (node.matches(HX) || node.querySelector(HX)))
          window.htmx.process(node);
        islands ||=
          node.matches("leptos-island") ||
          !!node.querySelector("leptos-island");
      }
    }
    if (islands) wake();
  }).observe(document.documentElement, { childList: true, subtree: true });
}
