// Leptos's island loader waits for idle time with no timeout, and Chrome 114
// to 123 can stop giving any after many whole loads in one tab: then no
// island starts. Every idle callback gets a 2 s timeout, as Leptos's own
// hydration script has. Loaded `defer` before <HydrationScripts/>, so it
// runs before the loader, a module script (tests/requests/home.rs checks
// the order). Remove once a Leptos release passes its own
// (docs/decisions.md).

// A block, so the const stays out of the page's global scope.
{
  const idle = window.requestIdleCallback;
  if (idle) {
    window.requestIdleCallback = (callback, options) =>
      idle.call(window, callback, { timeout: 2000, ...options });
  }
}
