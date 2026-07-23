(() => {
    const STORAGE = {
        refresh: "echonet.report.refreshSeconds",
        theme: "echonet.report.theme",
        filter: "echonet.report.filter",
        scope: "echonet.report.filterScope",
        density: "echonet.report.density",
        hashMode: "echonet.report.hashMode",
        sortPrefix: "echonet.report.sort.",
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

        if (theme !== "risk") {
            html.style.removeProperty("--accent");
            html.style.removeProperty("--accent2");
        }
    }

    function getDefaultTheme() {
        return "risk";
    }

    function nextTheme(current) {
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

        const stats = $("filterStats");
        if (stats) {
            const all = countVisibleRows(document);
            stats.textContent = all.total === 0 ? "" : `${all.visible}/${all.total} rows`;
        }
    }

    // ───────── Filtering + match highlighting ─────────

    function clearHighlightsIn(root) {
        root.querySelectorAll("mark.matchHighlight").forEach((m) => {
            const parent = m.parentNode;
            if (!parent) return;
            parent.replaceChild(document.createTextNode(m.textContent || ""), m);
            parent.normalize();
        });
    }

    function highlightMatchesIn(root, query) {
        if (!query) return;
        const q = query.toLowerCase();
        const SKIP_TAGS = new Set(["script", "style", "mark", "pre", "code"]);
        const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
            acceptNode(node) {
                const parent = node.parentNode;
                if (!parent) return NodeFilter.FILTER_REJECT;
                if (SKIP_TAGS.has((parent.nodeName || "").toLowerCase())) return NodeFilter.FILTER_REJECT;
                if (parent.classList && parent.classList.contains("matchHighlight")) return NodeFilter.FILTER_REJECT;
                if (!node.nodeValue) return NodeFilter.FILTER_REJECT;
                return NodeFilter.FILTER_ACCEPT;
            },
        });

        const targets = [];
        let n;
        while ((n = walker.nextNode())) {
            const t = n.nodeValue || "";
            if (!t.toLowerCase().includes(q)) continue;
            targets.push(n);
        }

        for (const node of targets) {
            const text = node.nodeValue || "";
            const frag = document.createDocumentFragment();
            const lower = text.toLowerCase();
            let i = 0;
            while (i < text.length) {
                const idx = lower.indexOf(q, i);
                if (idx < 0) {
                    frag.appendChild(document.createTextNode(text.slice(i)));
                    break;
                }
                if (idx > i) frag.appendChild(document.createTextNode(text.slice(i, idx)));
                const mark = document.createElement("mark");
                mark.className = "matchHighlight";
                mark.textContent = text.slice(idx, idx + q.length);
                frag.appendChild(mark);
                i = idx + q.length;
            }
            node.parentNode.replaceChild(frag, node);
        }
    }

    function filterAll(query, scope) {
        const q = normalize(query);
        const activeScope = scope || "all";

        const containers = document.querySelectorAll(".filterable[data-filter-scope]");
        containers.forEach((root) => {
            // Always clear stale highlights — even when not filtering this scope —
            // so flipping from "filter all" → "filter scope" doesn't leave orphans.
            clearHighlightsIn(root);

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
                if (match && q) highlightMatchesIn(tr, q);
            });

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
                    if (id === "pending-txs") d.open = false;
                }

                d.addEventListener("toggle", () => {
                    try {
                        localStorage.setItem(key, d.open ? "1" : "0");
                    } catch (_) { }
                });
            });
        } catch (_) { }
    }

    // ───────── Toast ─────────

    function toast(msg, kind) {
        let el = document.getElementById("toast");
        if (!el) {
            el = document.createElement("div");
            el.id = "toast";
            el.innerHTML = '<span class="toastDot" aria-hidden="true"></span><span class="toastText"></span>';
            document.body.appendChild(el);
        }
        el.className = "";
        if (kind === "success" || kind === "error" || kind === "info") {
            el.classList.add(kind);
        }
        el.querySelector(".toastText").textContent = msg;
        requestAnimationFrame(() => {
            el.style.opacity = "1";
            el.style.transform = "translateY(0)";
        });
        window.clearTimeout(el._t);
        el._t = window.setTimeout(() => {
            el.style.opacity = "0";
            el.style.transform = "translateY(6px)";
        }, 1300);
    }

    async function copyText(text, { silent = false } = {}) {
        const t = (text || "").toString();
        if (!t) return false;
        try {
            await navigator.clipboard.writeText(t);
            if (!silent) toast("Copied", "success");
            return true;
        } catch (_) { }
        const ta = document.createElement("textarea");
        ta.value = t;
        ta.style.position = "fixed";
        ta.style.left = "-1000px";
        ta.style.top = "-1000px";
        document.body.appendChild(ta);
        ta.focus();
        ta.select();
        let ok = false;
        try {
            ok = document.execCommand("copy");
            if (ok && !silent) toast("Copied", "success");
            else if (!silent) toast("Copy failed", "error");
        } catch (_) {
            if (!silent) toast("Copy failed", "error");
        } finally {
            document.body.removeChild(ta);
        }
        return ok;
    }

    function initCopyButtons() {
        document.querySelectorAll("button.copyBtn[data-copy]").forEach((btn) => {
            btn.addEventListener("click", () => copyText(btn.getAttribute("data-copy")));
        });
    }

    function initHashClickToCopy() {
        document.querySelectorAll(".hash").forEach((el) => {
            el.addEventListener("click", (e) => {
                if (window.getSelection && (window.getSelection().toString() || "").trim()) return;
                e.preventDefault();
                e.stopPropagation();
                const t = el.getAttribute("data-full-hash") || el.getAttribute("title") || el.textContent || "";
                copyText(t.trim());
            });
            el.setAttribute("role", "button");
            el.setAttribute("tabindex", "0");
            el.addEventListener("keydown", (e) => {
                if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    const t = el.getAttribute("data-full-hash") || el.getAttribute("title") || el.textContent || "";
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
        if (projectId) lines.push(`resource.labels.project_id="${projectId}"`);
        if (location) lines.push(`resource.labels.location="${location}"`);
        if (cluster) lines.push(`resource.labels.cluster_name="${cluster}"`);
        if (namespace) lines.push(`resource.labels.namespace_name="${namespace}"`);

        if (txHash) {
            const h = txHash.trim();
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
        const base = $("logsBaseLink");
        if (base) base.href = buildGcpLogsUrl();

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

        const focusKey = "echonet.report.filterFocused";
        input.addEventListener("focus", () => sessionStorage.setItem(focusKey, "1"));
        input.addEventListener("blur", () => sessionStorage.setItem(focusKey, "0"));
        if (sessionStorage.getItem(focusKey) === "1") {
            setTimeout(() => {
                input.focus();
                const v = input.value || "";
                input.setSelectionRange(v.length, v.length);
            }, 0);
        }
    }

    function initCopyVisibleHashes() {
        const btn = $("copyVisibleBtn");
        if (!btn) return;
        const scopeSelect = $("scopeSelect");

        function collectHashesWithin(root) {
            const out = [];
            root.querySelectorAll("tr[data-search]:not(.isHidden) .hash").forEach((el) => {
                const t = (el.getAttribute("data-full-hash") || el.getAttribute("title") || el.textContent || "").trim();
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
                toast("No visible hashes", "info");
                return;
            }
            copyText(uniq.join("\n"), { silent: true });
            toast(`Copied ${uniq.length} hashes`, "success");
        });
    }

    function initRefresh() {
        const select = $("refreshSelect");
        const btn = $("refreshNowBtn");
        const liveDot = $("liveIndicator");
        if (!select) return;

        const params = qs();
        const urlRefresh = params.get("refresh");
        const saved = localStorage.getItem(STORAGE.refresh) || "off";
        select.value = (urlRefresh != null ? urlRefresh : saved) || "off";

        function applyLiveState(v) {
            if (!liveDot) return;
            if (v === "off") liveDot.classList.remove("live");
            else liveDot.classList.add("live");
        }
        applyLiveState(select.value);

        let timer = null;
        function setTimer(v) {
            if (timer) window.clearInterval(timer);
            timer = null;
            applyLiveState(v);
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
            toast(`Theme: ${nxt}`, "info");
        });
    }

    function initDensityToggle() {
        const btn = $("densityBtn");
        if (!btn) return;
        const saved = localStorage.getItem(STORAGE.density) || "comfortable";
        function apply(mode) {
            document.body.setAttribute("data-density", mode);
            btn.setAttribute("aria-pressed", mode === "compact" ? "true" : "false");
            btn.textContent = mode === "compact" ? "Comfortable" : "Compact";
        }
        apply(saved);
        btn.addEventListener("click", () => {
            const cur = document.body.getAttribute("data-density") || "comfortable";
            const nxt = cur === "compact" ? "comfortable" : "compact";
            localStorage.setItem(STORAGE.density, nxt);
            apply(nxt);
        });
    }

    function shortenHash(full) {
        if (!full) return "";
        if (full.length <= 18) return full;
        return `${full.slice(0, 10)}…${full.slice(-8)}`;
    }

    function initHashModeToggle() {
        const btn = $("hashModeBtn");
        if (!btn) return;
        const saved = localStorage.getItem(STORAGE.hashMode) || "full";
        function apply(mode) {
            document.body.setAttribute("data-hash-mode", mode);
            btn.setAttribute("aria-pressed", mode === "short" ? "true" : "false");
            btn.textContent = mode === "short" ? "Full hashes" : "Short hashes";
            document.querySelectorAll(".hash[data-full-hash]").forEach((el) => {
                const full = el.getAttribute("data-full-hash") || "";
                if (!full) return;
                el.textContent = mode === "short" ? shortenHash(full) : full;
            });
            // Re-run filter so highlights re-apply to new text content
            const input = $("filterInput");
            const scopeSelect = $("scopeSelect");
            if (input) filterAll(input.value || "", scopeSelect ? scopeSelect.value : "all");
        }
        apply(saved);
        btn.addEventListener("click", () => {
            const cur = document.body.getAttribute("data-hash-mode") || "full";
            const nxt = cur === "short" ? "full" : "short";
            localStorage.setItem(STORAGE.hashMode, nxt);
            apply(nxt);
        });
    }

    function initExpandCollapseAll() {
        const expandBtn = $("expandAllBtn");
        const collapseBtn = $("collapseAllBtn");
        const all = () => document.querySelectorAll('details.sectionDetails[data-section-details="1"]');
        if (expandBtn) {
            expandBtn.addEventListener("click", () => {
                all().forEach((d) => { d.open = true; });
            });
        }
        if (collapseBtn) {
            collapseBtn.addEventListener("click", () => {
                all().forEach((d) => { d.open = false; });
            });
        }
    }

    function initBlockDumpPersistence() {
        const num = $("blockDumpNumber");
        const kind = $("blockDumpKind");
        if (!num && !kind) return;

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

        if (num) {
            num.addEventListener("input", () => {
                try { localStorage.setItem(STORAGE.blockDumpNumber, num.value || ""); } catch (_) { }
            });
        }
        if (kind) {
            kind.addEventListener("change", () => {
                try { localStorage.setItem(STORAGE.blockDumpKind, kind.value || ""); } catch (_) { }
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
        const risk = clamp01(document.body?.dataset?.echonetRevertRisk);
        const green = { r: 52, g: 156, b: 98 };
        const red = { r: 196, g: 56, b: 56 };
        const r = lerp(green.r, red.r, risk);
        const g = lerp(green.g, red.g, risk);
        const b = lerp(green.b, red.b, risk);
        const accent = `rgb(${r} ${g} ${b})`;
        document.documentElement.style.setProperty("--accent", accent);
        document.documentElement.style.setProperty("--accent2", accent);
    }

    function applyHealthDot() {
        const dot = $("healthDot");
        if (!dot) return;
        const pct = Number(document.body?.dataset?.echonetRevertRatePct || "0");
        // Severity: any non-empty bad metric tile turns the dot bad;
        // any non-empty warn-only state shows warn; else ok.
        const hasBad = document.querySelector(".metricTile.bad") != null;
        const hasWarn = document.querySelector(".metricTile.warn") != null;
        dot.classList.remove("ok", "warn", "bad");
        if (hasBad || pct >= 1) dot.classList.add("bad");
        else if (hasWarn || pct > 0) dot.classList.add("warn");
        // Default visual is ok (set by CSS)
        const title = hasBad || pct >= 1
            ? `Critical · echonet revert ${pct}%`
            : (hasWarn || pct > 0)
                ? `Warning · echonet revert ${pct}%`
                : `Healthy · echonet revert ${pct}%`;
        dot.setAttribute("title", title);
    }

    function applyProgressBar() {
        const fill = $("progressBarFill");
        if (!fill) return;
        const blocksText = document.querySelector('#progressNowLabel')?.textContent?.trim() || "";
        const startText = document.querySelector('#progressStartLabel')?.textContent?.trim() || "";
        const start = Number(startText);
        const now = Number(blocksText);
        if (!Number.isFinite(start) || !Number.isFinite(now) || start <= 0 || now <= 0) {
            const wrap = $("progressBarWrap");
            if (wrap) wrap.style.display = "none";
            return;
        }
        if (now <= start) {
            fill.style.width = "0%";
            return;
        }
        // Show progress relative to a 200-block "window" so the bar moves visibly.
        // (We don't have a target block number, so this is just a visual heartbeat.)
        const span = Math.max(1, now - start);
        const denom = Math.max(span, 200);
        const pct = Math.min(100, (span / denom) * 100);
        fill.style.width = `${pct.toFixed(1)}%`;

        const meta = $("progressMeta");
        if (meta) meta.textContent = `+${span} blocks since current start`;
    }

    function applyRiskGauge() {
        const fill = $("riskGaugeFill");
        const marker = $("riskGaugeMarker");
        const value = $("revertRateValue");
        if (!fill || !marker) return;
        const pct = Number(document.body?.dataset?.echonetRevertRatePct || "0");
        const clamped = Math.max(0, Math.min(100, pct));
        // The gauge visualizes 0% .. 1% — anything >= 1% pegs the marker at the right.
        const ratio = Math.min(1, clamped / 1.0);
        marker.style.left = `${(ratio * 100).toFixed(2)}%`;
        // Mask the unreached portion with a subtle overlay.
        fill.style.width = `${100 - ratio * 100}%`;
        fill.style.left = "auto";
        fill.style.right = "0";
        fill.style.inset = `0 0 0 auto`;
        if (value) value.textContent = (Math.round(pct * 1000) / 1000).toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
    }

    function applyStackedTxBar() {
        const okEl = $("stackedCommitted");
        const pendEl = $("stackedPending");
        if (!okEl || !pendEl) return;
        // Read totals out of the legend "mono" spans for robustness.
        const tile = okEl.closest(".tile");
        if (!tile) return;
        const monos = tile.querySelectorAll(".legendRow .mono");
        let committed = 0;
        let pending = 0;
        if (monos.length >= 2) {
            committed = Number((monos[0].textContent || "").replace(/[^0-9.-]/g, "")) || 0;
            pending = Number((monos[1].textContent || "").replace(/[^0-9.-]/g, "")) || 0;
        }
        const total = committed + pending;
        if (total <= 0) {
            okEl.style.width = "0%";
            pendEl.style.width = "0%";
            return;
        }
        okEl.style.width = `${(committed / total * 100).toFixed(2)}%`;
        pendEl.style.width = `${(pending / total * 100).toFixed(2)}%`;
    }

    function applyBarChartFills() {
        document.querySelectorAll("[data-bar-section]").forEach((section) => {
            const fills = section.querySelectorAll("[data-bar-count]");
            let max = 0;
            fills.forEach((f) => {
                const n = Number(f.getAttribute("data-bar-count") || "0");
                if (n > max) max = n;
            });
            fills.forEach((f) => {
                const n = Number(f.getAttribute("data-bar-count") || "0");
                const pct = max > 0 ? (n / max) * 100 : 0;
                f.style.width = `${pct.toFixed(1)}%`;
            });
        });
    }

    function initRevertBarRowClicks() {
        document.querySelectorAll(".barRow[data-scroll-to]").forEach((btn) => {
            btn.addEventListener("click", () => {
                const target = btn.getAttribute("data-scroll-to");
                if (!target) return;
                const el = document.getElementById(target);
                if (!el) return;
                openCollapsibleSection(el);
                el.scrollIntoView({ behavior: "smooth", block: "start" });

                // Briefly highlight the matching group inside the destination section.
                const groupName = btn.getAttribute("data-filter-text") || "";
                if (groupName) {
                    const summaries = el.querySelectorAll("details.revertGroup summary .mono");
                    summaries.forEach((s) => {
                        if ((s.textContent || "").trim() === groupName) {
                            const det = s.closest("details");
                            if (det) {
                                det.open = true;
                                det.scrollIntoView({ behavior: "smooth", block: "center" });
                                det.classList.remove("flash");
                                // Restart the animation by forcing a reflow.
                                // eslint-disable-next-line no-unused-expressions
                                det.offsetWidth;
                                det.classList.add("flash");
                            }
                        }
                    });
                }
            });
        });
    }

    function initScrollLinks() {
        function scrollToId(id) {
            const el = document.getElementById(id);
            if (!el) return;
            openCollapsibleSection(el);
            el.scrollIntoView({ behavior: "smooth", block: "start" });
        }

        document.querySelectorAll("[data-scroll-to]").forEach((el) => {
            const id = el.getAttribute("data-scroll-to");
            if (!id) return;

            el.addEventListener("click", (e) => {
                // Allow internal handlers (e.g. revert bar rows) to also act.
                if (el.classList.contains("barRow")) return;
                e.preventDefault();
                scrollToId(id);
            });
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
                try { sessionStorage.setItem(key, String(window.scrollY || 0)); } catch (_) { }
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

    function tableSortKey(table) {
        const cardSummary = table.closest("section[id]");
        const groupSummary = table.closest("details.revertGroup")?.querySelector("summary .mono")?.textContent || "";
        const sectionId = cardSummary ? cardSummary.id : "";
        const tableName = table.getAttribute("data-table") || "";
        return `${STORAGE.sortPrefix}${sectionId}::${tableName}::${groupSummary}`;
    }

    function sortRows(tbody, idx, type, dir) {
        const rows = Array.from(tbody.querySelectorAll("tr"));
        rows.sort((a, b) => {
            const av = getCellValue(a, idx);
            const bv = getCellValue(b, idx);
            if (type === "num") {
                const an = tryParseNumber(av);
                const bn = tryParseNumber(bv);
                const ax = an == null ? Number.NEGATIVE_INFINITY : an;
                const bx = bn == null ? Number.NEGATIVE_INFINITY : bn;
                return dir === "asc" ? ax - bx : bx - ax;
            }
            const an = tryParseNumber(av);
            const bn = tryParseNumber(bv);
            if (type === "auto" && an != null && bn != null) {
                return dir === "asc" ? an - bn : bn - an;
            }
            const as = normalize(av);
            const bs = normalize(bv);
            if (as < bs) return dir === "asc" ? -1 : 1;
            if (as > bs) return dir === "asc" ? 1 : -1;
            return 0;
        });
        rows.forEach((tr) => tbody.appendChild(tr));
    }

    function initSortableTables() {
        document.querySelectorAll("table").forEach((table) => {
            const thead = table.querySelector("thead");
            const tbody = table.querySelector("tbody");
            if (!thead || !tbody) return;
            const headers = thead.querySelectorAll("th.sortable");
            if (!headers.length) return;

            const storeKey = tableSortKey(table);
            let restored = null;
            try {
                const raw = localStorage.getItem(storeKey);
                if (raw) restored = JSON.parse(raw);
            } catch (_) { }

            headers.forEach((th, idx) => {
                th.setAttribute("role", "button");
                th.setAttribute("tabindex", "0");
                const sortType = th.getAttribute("data-sort") || "auto";

                function applyDir(dir) {
                    headers.forEach((h) => {
                        if (h !== th) {
                            h.removeAttribute("data-sort-dir");
                            h.removeAttribute("aria-sort");
                        }
                    });
                    th.setAttribute("data-sort-dir", dir);
                    th.setAttribute("aria-sort", dir === "asc" ? "ascending" : "descending");
                    sortRows(tbody, idx, sortType, dir);
                }

                function doSort() {
                    const cur = th.getAttribute("data-sort-dir") || "none";
                    const next = cur === "asc" ? "desc" : "asc";
                    applyDir(next);
                    try {
                        localStorage.setItem(storeKey, JSON.stringify({ idx, type: sortType, dir: next }));
                    } catch (_) { }
                }

                th.addEventListener("click", doSort);
                th.addEventListener("keydown", (e) => {
                    if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        doSort();
                    }
                });

                if (restored && restored.idx === idx) {
                    // Re-apply persisted sort to the header that matches by index.
                    applyDir(restored.dir);
                }
            });
        });
    }

    // ───────── Sidenav scrollspy ─────────

    function initSidenavScrollspy() {
        const items = Array.from(document.querySelectorAll(".sidenavItem[data-nav]"));
        if (!items.length) return;

        // A plain href jump lands on a collapsed section (e.g. default-collapsed
        // pending) with no table shown; open the target section on click too.
        items.forEach((a) => {
            a.addEventListener("click", () => {
                const el = document.getElementById(a.getAttribute("data-nav"));
                if (el) openCollapsibleSection(el);
            });
        });

        // Sections in DOM (vertical) order — required for the linear scan below.
        const sections = items
            .map((a) => ({ id: a.getAttribute("data-nav"), a, el: document.getElementById(a.getAttribute("data-nav")) }))
            .filter((x) => x.el);
        if (!sections.length) return;

        const setActive = (id) => {
            items.forEach((a) => a.classList.toggle("active", a.getAttribute("data-nav") === id));
        };

        function activationLine() {
            const raw = getComputedStyle(document.documentElement).getPropertyValue("--topbarH");
            const h = parseInt(raw, 10);
            return (Number.isFinite(h) ? h : 78) + 24;
        }

        let ticking = false;
        function update() {
            ticking = false;
            const line = activationLine();

            // The active section is the last one whose top has scrolled past the line.
            let currentId = sections[0].id;
            for (const s of sections) {
                if (s.el.getBoundingClientRect().top - line <= 0) currentId = s.id;
                else break;
            }

            // At the very bottom of the page, force-select the last section so
            // short trailing sections can still become active.
            const atBottom =
                window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 2;
            if (atBottom) currentId = sections[sections.length - 1].id;

            setActive(currentId);
        }

        window.addEventListener(
            "scroll",
            () => {
                if (ticking) return;
                ticking = true;
                window.requestAnimationFrame(update);
            },
            { passive: true }
        );
        window.addEventListener("resize", update);
        // Instant feedback when a nav item is clicked (before the smooth scroll settles).
        items.forEach((a) => a.addEventListener("click", () => setActive(a.getAttribute("data-nav"))));

        update();
    }

    // ───────── CSV export per section ─────────

    function csvEscape(value) {
        const s = (value == null ? "" : String(value));
        if (/[",\n\r]/.test(s)) {
            return `"${s.replace(/"/g, '""')}"`;
        }
        return s;
    }

    function tableToCsv(table) {
        const lines = [];
        const headerCells = table.querySelectorAll("thead th");
        const headers = Array.from(headerCells).map((th) => (th.textContent || "").trim());
        if (headers.length && headers[headers.length - 1] === "") headers.pop();
        lines.push(headers.map(csvEscape).join(","));

        table.querySelectorAll("tbody tr:not(.isHidden)").forEach((tr) => {
            const cols = Array.from(tr.children).slice(0, headers.length).map((td) => {
                // Prefer the full hash from data attribute if present.
                const hashEl = td.querySelector(".hash[data-full-hash]");
                if (hashEl) return hashEl.getAttribute("data-full-hash") || (td.textContent || "").trim();
                // Avoid copying nested <pre> blobs verbatim; collapse to one line.
                return (td.textContent || "").trim().replace(/\s+/g, " ");
            });
            lines.push(cols.map(csvEscape).join(","));
        });
        return lines.join("\n");
    }

    function initSectionCsvExport() {
        document.querySelectorAll(".sectionExportBtn[data-export-table]").forEach((btn) => {
            btn.addEventListener("click", (e) => {
                e.preventDefault();
                e.stopPropagation();
                const name = btn.getAttribute("data-export-table");
                const tables = document.querySelectorAll(`table[data-table="${CSS.escape(name)}"]`);
                if (!tables.length) {
                    toast("No data to export", "info");
                    return;
                }
                const parts = [];
                tables.forEach((t, i) => {
                    if (i > 0) parts.push(""); // blank separator
                    parts.push(tableToCsv(t));
                });
                const csv = parts.join("\n");
                copyText(csv, { silent: true });
                toast(`Copied ${name} as CSV`, "success");
            });
        });
    }

    // ───────── Generated-at "ago" timer ─────────

    function parseGeneratedAt() {
        const raw = (document.body?.dataset?.generatedAt || "").trim();
        if (!raw) return null;
        // Try ISO; if missing TZ, treat as UTC.
        let s = raw;
        if (!/Z|[+-]\d\d:?\d\d$/.test(s)) s = s + "Z";
        const t = Date.parse(s);
        if (!Number.isFinite(t)) return null;
        return t;
    }

    function formatAgo(ms) {
        if (ms < 0) ms = 0;
        const sec = Math.floor(ms / 1000);
        if (sec < 5) return "just now";
        if (sec < 60) return `${sec}s ago`;
        const min = Math.floor(sec / 60);
        if (min < 60) return `${min}m ago`;
        const hr = Math.floor(min / 60);
        if (hr < 48) return `${hr}h ${min % 60}m ago`;
        const day = Math.floor(hr / 24);
        return `${day}d ${hr % 24}h ago`;
    }

    function initAgoTimer() {
        const chip = $("agoChip");
        if (!chip) return;
        const t0 = parseGeneratedAt();
        if (t0 == null) {
            chip.style.display = "none";
            return;
        }
        const tick = () => {
            chip.textContent = formatAgo(Date.now() - t0);
        };
        tick();
        window.setInterval(tick, 1000);
    }

    // ───────── Shortcuts overlay + keyboard ─────────

    function initShortcutsOverlay() {
        const overlay = $("shortcutsOverlay");
        const open = $("shortcutsBtn");
        const close = $("shortcutsClose");
        if (!overlay) return;

        const show = () => {
            overlay.hidden = false;
            (close || open)?.focus();
        };
        const hide = () => { overlay.hidden = true; };

        if (open) open.addEventListener("click", show);
        if (close) close.addEventListener("click", hide);
        overlay.addEventListener("click", (e) => {
            if (e.target === overlay) hide();
        });
        // Expose so other key handlers can dismiss it.
        overlay._hide = hide;
        overlay._show = show;
        overlay._toggle = () => { overlay.hidden ? show() : hide(); };
    }

    function initKeyboard() {
        const input = $("filterInput");
        const scopeSelect = $("scopeSelect");
        const overlay = $("shortcutsOverlay");

        // "g" then a letter — jump shortcuts.
        let gPending = false;
        let gTimer = null;

        function clearGPending() {
            gPending = false;
            if (gTimer) { window.clearTimeout(gTimer); gTimer = null; }
        }

        const JUMPS = {
            o: "overview",
            p: "pending-txs",
            e: "gateway-errors",
            r: "reverts-mainnet",
            s: "resync-triggers",
            l: "l2-gas-mismatches",
            b: "block-hash-mismatches",
            t: "tx-commitment-mismatches",
        };

        // Track double-r for "reverts-echonet".
        let lastJump = null;
        let lastJumpAt = 0;

        function jumpToId(id) {
            const el = document.getElementById(id);
            if (!el) return;
            openCollapsibleSection(el);
            el.scrollIntoView({ behavior: "smooth", block: "start" });
        }

        window.addEventListener("keydown", (e) => {
            if (e.defaultPrevented) return;
            const tag = (e.target && e.target.tagName) ? e.target.tagName.toLowerCase() : "";
            const inTypingContext = tag === "input" || tag === "textarea" || tag === "select";
            const isMeta = e.ctrlKey || e.metaKey || e.altKey;

            // Filter & overlay shortcuts work outside typing context (except Esc).
            if (e.key === "Escape") {
                if (overlay && !overlay.hidden) {
                    e.preventDefault();
                    overlay._hide && overlay._hide();
                    return;
                }
                if (input && (document.activeElement === input || input.value)) {
                    input.value = "";
                    localStorage.setItem(STORAGE.filter, "");
                    filterAll("", scopeSelect ? scopeSelect.value : "all");
                    input.blur();
                    toast("Filter cleared", "info");
                }
                return;
            }

            if (inTypingContext || isMeta) return;

            if (e.key === "/") {
                if (input) {
                    e.preventDefault();
                    input.focus();
                    input.select();
                }
                clearGPending();
                return;
            }

            if (e.key === "?" || (e.shiftKey && e.key === "/")) {
                e.preventDefault();
                overlay && overlay._toggle && overlay._toggle();
                clearGPending();
                return;
            }

            const k = e.key.toLowerCase();

            if (gPending) {
                clearGPending();
                if (k === "r") {
                    // Double-r within 600ms → echonet reverts.
                    const now = Date.now();
                    if (lastJump === "r" && now - lastJumpAt < 600) {
                        jumpToId("reverts-echonet");
                        lastJump = null;
                        return;
                    }
                    lastJump = "r";
                    lastJumpAt = now;
                }
                const target = JUMPS[k];
                if (target) {
                    e.preventDefault();
                    jumpToId(target);
                }
                return;
            }

            if (k === "g") {
                gPending = true;
                gTimer = window.setTimeout(clearGPending, 800);
                e.preventDefault();
                return;
            }

            if (k === "r") {
                // Second tap of the "G R R" chord jumps to echonet reverts;
                // it must not fall through to the refresh (page reload).
                const now = Date.now();
                if (lastJump === "r" && now - lastJumpAt < 600) {
                    e.preventDefault();
                    jumpToId("reverts-echonet");
                    lastJump = null;
                    return;
                }
                const btn = $("refreshNowBtn");
                if (btn) {
                    e.preventDefault();
                    btn.click();
                }
                return;
            }

            if (k === "e") {
                const btn = $("expandAllBtn");
                if (btn) { e.preventDefault(); btn.click(); }
                return;
            }
            if (k === "c") {
                const btn = $("collapseAllBtn");
                if (btn) { e.preventDefault(); btn.click(); }
                return;
            }
            if (k === "d") {
                const btn = $("densityBtn");
                if (btn) { e.preventDefault(); btn.click(); }
                return;
            }
            if (k === "h") {
                const btn = $("hashModeBtn");
                if (btn) { e.preventDefault(); btn.click(); }
                return;
            }
            if (k === "t") {
                const btn = $("themeToggleBtn");
                if (btn) { e.preventDefault(); btn.click(); }
                return;
            }
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
        initDensityToggle();
        initHashModeToggle();
        initExpandCollapseAll();
        initBlockDumpPersistence();
        initSectionCollapsePersistence();
        initScrollLinks();
        initSortableTables();
        initSectionParamScroll();
        initHashOpen();
        initRestoreScrollPosition();
        initSidenavScrollspy();
        initRevertBarRowClicks();
        initSectionCsvExport();
        initAgoTimer();
        initShortcutsOverlay();
        initKeyboard();

        // Visualizations (rely on data already on the page).
        applyHealthDot();
        applyProgressBar();
        applyRiskGauge();
        applyStackedTxBar();
        applyBarChartFills();

        updateTableMeta();
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", boot);
    } else {
        boot();
    }
})();
