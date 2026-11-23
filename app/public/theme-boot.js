// Before the app loads: the theme the user chose in Settings (else Windows'),
// and whether the window was translucent last time (the app knows only once
// it has asked), so the launch animation and the first frame match it. A file
// of its own: the content security policy allows no inline scripts.
(function () {
  try {
    var pref = localStorage.getItem("ks.theme");
    var dark = pref === "dark" || (pref !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    var mica = localStorage.getItem("ks.mica");
    if (mica === "true" || mica === "false") document.documentElement.dataset.mica = mica;
  } catch (e) {
    /* storage unavailable: the app sets the theme once it loads */
  }
})();
