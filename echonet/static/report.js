(() => {
    const STORAGE = {
        refresh: "echonet.report.refreshSeconds",
        theme: "echonet.report.theme",
        filter: "echonet.report.filter",
        scope: "echonet.report.filterScope",
        blockDumpNumber: "echonet.report.blockDumpNumber",
        blockDumpKind: "echonet.report.blockDumpKind",
    };

    function $(id) {
        return document.getElementById(id);
    }

    function qs() {
        return new URLSearchParams(window.location.search || "");
    }

    function applyTheme(theme) {
        const html = document.documentElement;
        if (theme === "light" || theme === "dark" || theme === "risk") {
            html.setAttribute("data-theme", theme);
        } else {
            html.removeAttribute("data-theme");
        }

        // "risk" sets --accent/--accent2 inline; clear when leaving it so dark/light
        // revert to CSS defaults immediately (no refresh required).
        if (theme !== "risk") {
            html.style.removeProperty("--accent");
            html.style.removeProperty("--accent2");
        }
    }

    function getDefaultTheme() {
        // Default: risk (accent derived from echonet-only revert rate).
        return "risk";
    }

    function nextTheme(current) {
        // Cycle: risk -> dark -> light -> risk
        if (current === "risk") return "dark";
        if (current === "dark") return "light";
        return "risk";
    }

    function normalize(s) {
        return (s || "").toString().toLowerCase().trim();
    }

    function setHidden(el, hidden) {
        if (!el) return;
        if (hidden) el.classList.add("isHidden");
        else el.classList.remove("isHidden");
    }

    function openCollapsibleSection(root) {
        if (!root) return;
        const tag = root.tagName ? root.tagName.toLowerCase() : "";
        if (tag === "details") {
            root.open = true;
            return;
        }
        const d =
            root.querySelector &&
            root.querySelector('details[data-section-details="1"], details.sectionDetails');
        if (d) d.open = true;
    }

    function countVisibleRows(root) {
        const rows = root.querySelectorAll("tr[data-search]");
        let total = 0;
        let visible = 0;
        rows.forEach((tr) => {
            total += 1;
            if (!tr.classList.contains("isHidden")) visible += 1;
        });
        return { total, visible };
    }

    function updateTableMeta() {
        // Tables (by data-table)
        document.querySelectorAll("[data-table-meta]").forEach((el) => {
            const name = el.getAttribute("data-table-meta");
            if (!name) return;
            const tables = document.querySelectorAll(`table[data-table="${CSS.escape(name)}"]`);
            let total = 0;
            let visible = 0;
            tables.forEach((t) => {
                const c = countVisibleRows(t);
                total += c.total;
                visible += c.visible;
            });
            el.textContent = total === 0 ? "" : `Showing ${visible}/${total} rows`;
        });

        // Global stats (all rows)
        const stats = $("filterStats");
        if (stats) {
            const all = countVisibleRows(document);
            stats.textContent = all.total === 0 ? "" : `${all.visible}/${all.total} rows`;
        }
    }

    function filterAll(query, scope) {
        const q = normalize(query);

        const activeScope = scope || "all";

        // Filter each filterable container independently. If scope doesn't match, leave it unfiltered.
        const containers = document.querySelectorAll(".filterable[data-filter-scope]");
        containers.forEach((root) => {
            const rootScope = root.getAttribute("data-filter-scope") || "";
            const doFilter = activeScope === "all" || activeScope === rootScope;
            const rows = root.querySelectorAll("tr[data-search]");
            rows.forEach((tr) => {
                if (!doFilter) {
                    setHidden(tr, false);
                    return;
                }
                const hay = normalize(tr.getAttribute("data-search"));
                const match = !q || hay.includes(q);
                setHidden(tr, !match);
            });

            // If root is a <details>, hide the entire group if it has no visible rows (only when filtering it).
            if (root.tagName && root.tagName.toLowerCase() === "details") {
                if (!doFilter) {
                    setHidden(root, false);
                } else {
                    const anyVisible = root.querySelector("tr[data-search]:not(.isHidden)") != null;
                    setHidden(root, !anyVisible);
                }
            }
        });

        updateTableMeta();
    }

    function initSectionCollapsePersistence() {
        // Persist section open/closed state across refreshes (like refresh dropdown).
        // Keyed by the <section id="..."> that wraps a `details.sectionDetails`.
        try {
            const prefix = "echonet.report.sectionOpen.";
            document.querySelectorAll('details.sectionDetails[data-section-details="1"]').forEach((d) => {
                const section = d.closest("section[id]");
                const id = section ? (section.id || "") : "";
                if (!id) return;

                const key = prefix + id;
                const saved = localStorage.getItem(key);
                if (saved === "1") d.open = true;
                else if (saved === "0") d.open = false;
                else {
                    // Defaults: everything open except pending txs.
                    if (id === "pending-txs") d.open = false;
                }

                d.addEventListener("toggle", () => {
                    try {
                        localStorage.setItem(key, d.open ? "1" : "0");
                    } catch (_) { }
                });
            });
        } catch (_) {
            // If localStorage is unavailable, just fall back to HTML defaults.
        }
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

    function initHashClickToCopy() {
        document.querySelectorAll(".hash").forEach((el) => {
            el.addEventListener("click", (e) => {
                // Avoid interfering with text selection.
                if (window.getSelection && (window.getSelection().toString() || "").trim()) return;
                e.preventDefault();
                e.stopPropagation();
                const t = el.getAttribute("title") || el.textContent || "";
                copyText(t.trim());
            });
            el.setAttribute("role", "button");
            el.setAttribute("tabindex", "0");
            el.addEventListener("keydown", (e) => {
                if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    const t = el.getAttribute("title") || el.textContent || "";
                    copyText(t.trim());
                }
            });
        });
    }

    function buildGcpLogsUrl({ txHash } = {}) {
        const b = document.body?.dataset || {};
        const projectId = (b.gcpProjectId || "").trim();
        const location = (b.gcpLocation || "").trim();
        const cluster = (b.gkeClusterName || "").trim();
        const namespace = (b.k8sNamespace || "").trim();
        const duration = (b.gcpLogsDuration || "").trim();

        const lines = [];
        lines.push('resource.type="k8s_container"');
        // These labels are harmless even if they don't match (they narrow search).
        if (projectId) lines.push(`resource.labels.project_id="${projectId}"`);
        if (location) lines.push(`resource.labels.location="${location}"`);
        if (cluster) lines.push(`resource.labels.cluster_name="${cluster}"`);
        if (namespace) lines.push(`resource.labels.namespace_name="${namespace}"`);

        if (txHash) {
            const h = txHash.trim();
            // A simple string search is good enough here.
            lines.push(`"${h}"`);
        }

        const q = lines.join("\n");
        const encoded = encodeURIComponent(q);
        const cursorTimestamp = encodeURIComponent(new Date().toISOString());
        const url =
            `https://console.cloud.google.com/logs/query;` +
            `query=${encoded};storageScope=project;cursorTimestamp=${cursorTimestamp};duration=${encodeURIComponent(duration)}` +
            `?project=${encodeURIComponent(projectId)}`;
        return url;
    }

    function initLogsLinks() {
        // Base namespace logs link.
        const base = $("logsBaseLink");
        if (base) {
            base.href = buildGcpLogsUrl();
        }

        // Per-tx logs links.
        document.querySelectorAll("a.logsLink[data-logs-tx]").forEach((a) => {
            const txHash = a.getAttribute("data-logs-tx") || "";
            a.href = buildGcpLogsUrl({ txHash });
        });
    }

    function initFilter() {
        const input = $("filterInput");
        if (!input) return;
        const scopeSelect = $("scopeSelect");

        const params = qs();
        const urlQ = params.get("q");
        const urlScope = params.get("scope");

        const savedQ = localStorage.getItem(STORAGE.filter) || "";
        const savedScope = localStorage.getItem(STORAGE.scope) || "all";

        // URL params win (shareable view), otherwise fall back to local storage.
        input.value = (urlQ != null ? urlQ : savedQ) || "";
        if (scopeSelect) scopeSelect.value = (urlScope != null ? urlScope : savedScope) || "all";

        filterAll(input.value, scopeSelect ? scopeSelect.value : "all");
        input.addEventListener("input", () => {
            localStorage.setItem(STORAGE.filter, input.value);
            filterAll(input.value, scopeSelect ? scopeSelect.value : "all");
        });
        if (scopeSelect) {
            scopeSelect.addEventListener("change", () => {
                localStorage.setItem(STORAGE.scope, scopeSelect.value);
                filterAll(input.value, scopeSelect.value);
            });
        }
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
                    filterAll("", scopeSelect ? scopeSelect.value : "all");
                    input.blur();
                    toast("Filter cleared");
                }
            }
        });
    }

    function initCopyVisibleHashes() {
        const btn = $("copyVisibleBtn");
        if (!btn) return;
        const scopeSelect = $("scopeSelect");

        function collectHashesWithin(root) {
            const out = [];
            root.querySelectorAll("tr[data-search]:not(.isHidden) .hash").forEach((el) => {
                const t = (el.getAttribute("title") || el.textContent || "").trim();
                if (t) out.push(t);
            });
            return out;
        }

        btn.addEventListener("click", () => {
            const scope = scopeSelect ? (scopeSelect.value || "all") : "all";
            let hashes = [];
            if (scope === "all") {
                hashes = collectHashesWithin(document);
            } else {
                document
                    .querySelectorAll(`.filterable[data-filter-scope="${CSS.escape(scope)}"]`)
                    .forEach((root) => {
                        hashes.push(...collectHashesWithin(root));
                    });
            }

            const uniq = Array.from(new Set(hashes));
            if (!uniq.length) {
                toast("No visible hashes");
                return;
            }
            copyText(uniq.join("\n"));
            toast(`Copied ${uniq.length} hashes`);
        });
    }

    function initRefresh() {
        const select = $("refreshSelect");
        const btn = $("refreshNowBtn");
        if (!select) return;

        const params = qs();
        const urlRefresh = params.get("refresh");
        const saved = localStorage.getItem(STORAGE.refresh) || "off";
        select.value = (urlRefresh != null ? urlRefresh : saved) || "off";

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
        const params = qs();
        const urlTheme = params.get("theme");
        let saved = (urlTheme != null ? urlTheme : (localStorage.getItem(STORAGE.theme) || getDefaultTheme())) || getDefaultTheme();
        // Migrate old "system" setting to the new default.
        if (saved === "system") {
            saved = getDefaultTheme();
            localStorage.setItem(STORAGE.theme, saved);
        }
        applyTheme(saved);
        if (saved === "risk") applyRiskAccent();
        btn.addEventListener("click", () => {
            let cur = localStorage.getItem(STORAGE.theme) || getDefaultTheme();
            if (cur === "system") cur = getDefaultTheme();
            const nxt = nextTheme(cur);
            localStorage.setItem(STORAGE.theme, nxt);
            applyTheme(nxt);
            if (nxt === "risk") applyRiskAccent();
            toast(`Theme: ${nxt}`);
        });
    }

    function initBlockDumpPersistence() {
        const num = $("blockDumpNumber");
        const kind = $("blockDumpKind");
        if (!num && !kind) return;

        // Restore (localStorage wins; no URL param support needed here since the form opens in a new tab).
        try {
            if (num) {
                const saved = localStorage.getItem(STORAGE.blockDumpNumber);
                if (saved != null && saved !== "") num.value = saved;
            }
            if (kind) {
                const saved = localStorage.getItem(STORAGE.blockDumpKind);
                if (saved != null && saved !== "") kind.value = saved;
            }
        } catch (_) { }

        // Preserve focus across auto-refresh reloads (best-effort), like filter input.
        if (num) {
            const focusKey = "echonet.report.blockDumpNumberFocused";
            num.addEventListener("focus", () => sessionStorage.setItem(focusKey, "1"));
            num.addEventListener("blur", () => sessionStorage.setItem(focusKey, "0"));
            if (sessionStorage.getItem(focusKey) === "1") {
                setTimeout(() => {
                    num.focus();
                    num.select();
                }, 0);
            }
        }

        // Persist changes.
        if (num) {
            num.addEventListener("input", () => {
                try {
                    localStorage.setItem(STORAGE.blockDumpNumber, num.value || "");
                } catch (_) { }
            });
        }
        if (kind) {
            kind.addEventListener("change", () => {
                try {
                    localStorage.setItem(STORAGE.blockDumpKind, kind.value || "");
                } catch (_) { }
            });
        }
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
            // If the target section is collapsed, open it before scrolling.
            openCollapsibleSection(el);
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

    function initRestoreScrollPosition() {
        const key = "echonet.report.scrollY";
        let ticking = false;
        window.addEventListener("scroll", () => {
            if (ticking) return;
            ticking = true;
            window.requestAnimationFrame(() => {
                try {
                    sessionStorage.setItem(key, String(window.scrollY || 0));
                } catch (_) { }
                ticking = false;
            });
        });

        const params = qs();
        const hasExplicitAnchor = (window.location.hash || "").length > 1 || params.get("section");
        if (hasExplicitAnchor) return;
        const raw = sessionStorage.getItem(key);
        const y = raw ? parseInt(raw, 10) : 0;
        if (Number.isFinite(y) && y > 0) {
            window.scrollTo({ top: y, behavior: "auto" });
        }
    }

    function initSectionParamScroll() {
        const params = qs();
        const section = params.get("section");
        if (!section) return;
        const el = document.getElementById(section);
        if (el) {
            // If the target section is collapsed, open it before scrolling.
            openCollapsibleSection(el);
            setTimeout(() => el.scrollIntoView({ behavior: "smooth", block: "start" }), 0);
        }
    }

    function initHashOpen() {
        const h = (window.location.hash || "").trim();
        if (!h || h === "#") return;
        const id = h.startsWith("#") ? h.slice(1) : h;
        if (!id) return;
        const el = document.getElementById(id);
        if (el) openCollapsibleSection(el);
    }

    function getCellValue(tr, idx) {
        const td = tr.children && tr.children[idx];
        if (!td) return "";
        return (td.textContent || "").trim();
    }

    function tryParseNumber(s) {
        const t = (s || "").replace(/,/g, "").trim();
        if (!t) return null;
        const n = Number(t);
        return Number.isFinite(n) ? n : null;
    }

    function initSortableTables() {
        document.querySelectorAll("table").forEach((table) => {
            const thead = table.querySelector("thead");
            const tbody = table.querySelector("tbody");
            if (!thead || !tbody) return;
            const headers = thead.querySelectorAll("th.sortable");
            if (!headers.length) return;

            headers.forEach((th, idx) => {
                th.setAttribute("role", "button");
                th.setAttribute("tabindex", "0");
                const sortType = th.getAttribute("data-sort") || "auto";

                function doSort() {
                    const cur = th.getAttribute("data-sort-dir") || "none";
                    const next = cur === "asc" ? "desc" : "asc";

                    // Reset siblings
                    headers.forEach((h) => {
                        if (h !== th) {
                            h.removeAttribute("data-sort-dir");
                            h.removeAttribute("aria-sort");
                        }
                    });

                    th.setAttribute("data-sort-dir", next);
                    th.setAttribute("aria-sort", next === "asc" ? "ascending" : "descending");

                    const rows = Array.from(tbody.querySelectorAll("tr"));
                    rows.sort((a, b) => {
                        const av = getCellValue(a, idx);
                        const bv = getCellValue(b, idx);

                        if (sortType === "num") {
                            const an = tryParseNumber(av);
                            const bn = tryParseNumber(bv);
                            const ax = an == null ? Number.NEGATIVE_INFINITY : an;
                            const bx = bn == null ? Number.NEGATIVE_INFINITY : bn;
                            return next === "asc" ? ax - bx : bx - ax;
                        }

                        // auto / str
                        const an = tryParseNumber(av);
                        const bn = tryParseNumber(bv);
                        if (sortType === "auto" && an != null && bn != null) {
                            return next === "asc" ? an - bn : bn - an;
                        }

                        const as = normalize(av);
                        const bs = normalize(bv);
                        if (as < bs) return next === "asc" ? -1 : 1;
                        if (as > bs) return next === "asc" ? 1 : -1;
                        return 0;
                    });

                    rows.forEach((tr) => tbody.appendChild(tr));
                }

                th.addEventListener("click", doSort);
                th.addEventListener("keydown", (e) => {
                    if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        doSort();
                    }
                });
            });
        });
    }

    function boot() {
        initCopyButtons();
        initHashClickToCopy();
        initLogsLinks();
        initFilter();
        initCopyVisibleHashes();
        initRefresh();
        initThemeToggle();
        initBlockDumpPersistence();
        initSectionCollapsePersistence();
        initScrollLinks();
        initSortableTables();
        initSectionParamScroll();
        initHashOpen();
        initRestoreScrollPosition();
        updateTableMeta();
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", boot);
    } else {
        boot();
    }
})();