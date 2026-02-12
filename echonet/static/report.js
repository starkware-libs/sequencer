(() => {
  const STORAGE = {
    refresh: "echonet.report.refreshSeconds",
    theme: "echonet.report.theme",
    filter: "echonet.report.filter",
  };

  function $(id) {
    return document.getElementById(id);
  }

  function applyTheme(theme) {
    const html = document.documentElement;
    if (theme === "light" || theme === "dark" || theme === "risk") {
      html.setAttribute("data-theme", theme);
    } else {
      html.removeAttribute("data-theme");
    }
  }

  function getDefaultTheme() {
    // Default: follow system.
    return "system";
  }

  function nextTheme(current) {
    // Cycle: system -> dark -> light -> risk -> system
    if (current === "system") return "dark";
    if (current === "dark") return "light";
    if (current === "light") return "risk";
    return "system";
  }

  function normalize(s) {
    return (s || "").toString().toLowerCase().trim();
  }

  function setHidden(el, hidden) {
    if (!el) return;
    if (hidden) el.classList.add("isHidden");
    else el.classList.remove("isHidden");
  }

  function filterAll(query) {
    const q = normalize(query);

    // Rows
    const rows = document.querySelectorAll("tr[data-search]");
    rows.forEach((tr) => {
      const hay = normalize(tr.getAttribute("data-search"));
      const match = !q || hay.includes(q);
      setHidden(tr, !match);
    });

    // Revert group <details> containers (hide entire group if none of its rows match)
    const groups = document.querySelectorAll("details.filterable[data-filter-scope='reverts']");
    groups.forEach((det) => {
      const groupRows = det.querySelectorAll("tr[data-search]");
      let any = false;
      groupRows.forEach((tr) => {
        if (!tr.classList.contains("isHidden")) any = true;
      });
      setHidden(det, !any);
    });
  }

  function toast(msg) {
    // Tiny toast without dependencies.
    let el = document.getElementById("toast");
    if (!el) {
      el = document.createElement("div");
      el.id = "toast";
      el.style.position = "fixed";
      el.style.bottom = "14px";
      el.style.right = "14px";
      el.style.padding = "10px 12px";
      el.style.borderRadius = "12px";
      el.style.border = "1px solid rgba(255,255,255,0.12)";
      el.style.background = "rgba(20, 24, 39, 0.92)";
      el.style.backdropFilter = "blur(10px)";
      el.style.color = "#e7eaf0";
      el.style.fontSize = "12px";
      el.style.fontFamily = "ui-sans-serif, system-ui";
      el.style.boxShadow = "0 18px 40px rgba(0,0,0,0.35)";
      el.style.zIndex = "999";
      el.style.opacity = "0";
      el.style.transform = "translateY(6px)";
      el.style.transition = "opacity 120ms ease, transform 120ms ease";
      document.body.appendChild(el);
    }
    el.textContent = msg;
    requestAnimationFrame(() => {
      el.style.opacity = "1";
      el.style.transform = "translateY(0)";
    });
    window.clearTimeout(el._t);
    el._t = window.setTimeout(() => {
      el.style.opacity = "0";
      el.style.transform = "translateY(6px)";
    }, 1100);
  }

  async function copyText(text) {
    const t = (text || "").toString();
    if (!t) return;
    try {
      await navigator.clipboard.writeText(t);
      toast("Copied");
      return;
    } catch (_) {
      // fall through
    }
    // Fallback
    const ta = document.createElement("textarea");
    ta.value = t;
    ta.style.position = "fixed";
    ta.style.left = "-1000px";
    ta.style.top = "-1000px";
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    try {
      document.execCommand("copy");
      toast("Copied");
    } catch (_) {
      toast("Copy failed");
    } finally {
      document.body.removeChild(ta);
    }
  }

  function initCopyButtons() {
    document.querySelectorAll("button.copyBtn[data-copy]").forEach((btn) => {
      btn.addEventListener("click", () => copyText(btn.getAttribute("data-copy")));
    });
  }

  function initFilter() {
    const input = $("filterInput");
    if (!input) return;
    const saved = localStorage.getItem(STORAGE.filter) || "";
    if (saved) input.value = saved;
    filterAll(input.value);
    input.addEventListener("input", () => {
      localStorage.setItem(STORAGE.filter, input.value);
      filterAll(input.value);
    });

    // Preserve focus across auto-refresh reloads (best-effort).
    const focusKey = "echonet.report.filterFocused";
    input.addEventListener("focus", () => sessionStorage.setItem(focusKey, "1"));
    input.addEventListener("blur", () => sessionStorage.setItem(focusKey, "0"));
    if (sessionStorage.getItem(focusKey) === "1") {
      // Defer to allow initial layout to settle.
      setTimeout(() => {
        input.focus();
        // Put caret at end.
        const v = input.value || "";
        input.setSelectionRange(v.length, v.length);
      }, 0);
    }

    // Keyboard shortcuts:
    // - "/" focuses filter (like many dashboards)
    // - "Escape" clears filter
    window.addEventListener("keydown", (e) => {
      if (e.defaultPrevented) return;
      const tag = (e.target && e.target.tagName) ? e.target.tagName.toLowerCase() : "";
      const inTypingContext = tag === "input" || tag === "textarea" || tag === "select";

      if (!inTypingContext && e.key === "/") {
        e.preventDefault();
        input.focus();
        input.select();
        return;
      }
      if (e.key === "Escape") {
        if (document.activeElement === input || input.value) {
          input.value = "";
          localStorage.setItem(STORAGE.filter, "");
          filterAll("");
          input.blur();
          toast("Filter cleared");
        }
      }
    });
  }

  function initRefresh() {
    const select = $("refreshSelect");
    const btn = $("refreshNowBtn");
    if (!select) return;

    const saved = localStorage.getItem(STORAGE.refresh) || "off";
    select.value = saved;

    let timer = null;
    function setTimer(v) {
      if (timer) window.clearInterval(timer);
      timer = null;
      if (v === "off") return;
      const secs = parseInt(v, 10);
      if (!Number.isFinite(secs) || secs <= 0) return;
      timer = window.setInterval(() => window.location.reload(), secs * 1000);
    }

    setTimer(select.value);
    select.addEventListener("change", () => {
      localStorage.setItem(STORAGE.refresh, select.value);
      setTimer(select.value);
    });

    if (btn) btn.addEventListener("click", () => window.location.reload());
  }

  function initThemeToggle() {
    const btn = $("themeToggleBtn");
    if (!btn) return;
    const saved = localStorage.getItem(STORAGE.theme) || getDefaultTheme();
    applyTheme(saved);
    if (saved === "risk") applyRiskAccent();
    btn.addEventListener("click", () => {
      const cur = localStorage.getItem(STORAGE.theme) || "system";
      const nxt = nextTheme(cur);
      localStorage.setItem(STORAGE.theme, nxt);
      applyTheme(nxt);
      if (nxt === "risk") applyRiskAccent();
      toast(`Theme: ${nxt}`);
    });
  }

  function clamp01(x) {
    const n = Number(x);
    if (!Number.isFinite(n)) return 0;
    if (n < 0) return 0;
    if (n > 1) return 1;
    return n;
  }

  function lerp(a, b, t) {
    return Math.round(a + (b - a) * t);
  }

  function applyRiskAccent() {
    // Map echonet-only revert rate -> accent color.
    // 0% => muted green; 5%+ => muted red.
    const risk = clamp01(document.body?.dataset?.echonetRevertRisk);

    // Muted (non-neon) endpoints.
    const green = { r: 52, g: 156, b: 98 }; // slightly stronger green
    const red = { r: 196, g: 56, b: 56 };   // slightly stronger red

    const r = lerp(green.r, red.r, risk);
    const g = lerp(green.g, red.g, risk);
    const b = lerp(green.b, red.b, risk);

    const accent = `rgb(${r} ${g} ${b})`;
    document.documentElement.style.setProperty("--accent", accent);
    // Keep any secondary glow aligned with the same accent (avoid hard banding).
    document.documentElement.style.setProperty("--accent2", accent);
  }

  function initScrollLinks() {
    function scrollToId(id) {
      const el = document.getElementById(id);
      if (!el) return;
      el.scrollIntoView({ behavior: "smooth", block: "start" });
    }

    document.querySelectorAll("[data-scroll-to]").forEach((el) => {
      const id = el.getAttribute("data-scroll-to");
      if (!id) return;

      el.addEventListener("click", () => scrollToId(id));
      el.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          scrollToId(id);
        }
      });
    });
  }

  function boot() {
    initCopyButtons();
    initFilter();
    initRefresh();
    initThemeToggle();
    initScrollLinks();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();

