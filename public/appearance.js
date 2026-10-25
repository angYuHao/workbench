/* 首帧之前，把上次算好的界面变量贴回 <html>。

   真实来源是本机的 settings 表，但那条读数要走 invoke（异步），React 首帧一定先用兜底值画完 ——
   存了深色的用户每次开机就要白一下。所以 styles/apply.ts 每次应用后把结果镜像进 localStorage，
   这里只做同步重放，不再算一遍颜色，两边不可能出现两套口径。

   CSP 是 script-src 'self'，所以这段必须是有 src 的外部脚本，不能写成 index.html 里的内联脚本。
   别在 tauri dev 里验证这件事：dev 的页面来自 vite，不吃 tauri.conf.json 里那份 CSP。 */
(function () {
  try {
    var raw = localStorage.getItem("appearance.cache.v1");
    if (!raw) return;
    var cache = JSON.parse(raw);
    if (!cache || !cache.tokens || !cache.mode) return;
    var root = document.documentElement;
    for (var name in cache.tokens) {
      if (Object.prototype.hasOwnProperty.call(cache.tokens, name)) {
        root.style.setProperty(name, cache.tokens[name]);
      }
    }
    root.dataset.mode = cache.mode;
    root.dataset.style = cache.styleId || "";
  } catch (e) {
    /* 镜像坏了就回落到 tokens.css 的兜底值，不值得为它挡渲染 */
  }
})();
