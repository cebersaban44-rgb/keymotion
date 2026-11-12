// Before the app loads: the theme the user chose in Settings (else Windows'),
// so the launch animation and the first frame match it. A file of its own:
// the content security policy allows no inline scripts.
(function () {
  try {
    var pref = localStorage.getItem("ks.theme");
    var dark = pref === "dark" || (pref !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
    document.documentElement.dataset.theme = dark ? "dark" : "light";
  } catch (e) {
    /* storage unavailable: the app sets the theme once it loads */
  }
})();
