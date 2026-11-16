(function () {
  "use strict";
  if (window.parent === window) return;
  document.addEventListener("click", function (event) {
    var open = event.target && event.target.closest && event.target.closest("[data-inkdos-open]");
    if (open) {
      event.preventDefault();
      window.parent.postMessage({ type: "inkdos:open-app", id: open.getAttribute("data-inkdos-open") }, location.origin);
      return;
    }
    var home = event.target && event.target.closest && event.target.closest("a.inkdos-hub-link, a.ink-home");
    if (home) {
      event.preventDefault();
      window.parent.postMessage({ type: "inkdos:close-app" }, location.origin);
    }
  });
})();