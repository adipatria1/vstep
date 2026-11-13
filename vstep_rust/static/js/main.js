document.addEventListener('DOMContentLoaded', () => {
    const form = document.getElementById('backtest-form');
    const btnRun = document.getElementById('btn-run');
    const btnRecalc = document.getElementById('btn-recalc');
    const cacheIndicator = document.getElementById('cache-indicator');

    const progressContainer = document.getElementById('progress-container');
    const progressLabel = document.getElementById('progress-label');
    const progressPercent = document.getElementById('progress-percent');
    const progressBar = document.getElementById('progress-bar');
    const milestoneAlert = document.getElementById('milestone-alert');
    const milestoneText = document.getElementById('milestone-text');

    const summaryBar = document.getElementById('summary-bar');
    const sumCoin = document.getElementById('sum-coin');
    const sumCandles = document.getElementById('sum-candles');
    const sumPrice = document.getElementById('sum-price');
    const sumStrats = document.getElementById('sum-strats');

    const ldTitle = document.getElementById('ld-title');
    const ldBody = document.getElementById('ld-body');
    const dtTitle = document.getElementById('dt-title');
    const dtBody = document.getElementById('dt-body');

    const auxContainer = document.getElementById('aux-container');
    const tradesBody = document.getElementById('trades-body');
    const positionsBody = document.getElementById('positions-body');

    const dataStatus = document.getElementById('data-status');

    let currentEventSource = null;
    let lastResult = null;

    // Check cache on input change
    function checkCache() {
        const symbol = document.getElementById('symbol').value.trim().toUpperCase();
        const interval = document.getElementById('interval').value;
        const days = document.getElementById('days').value;

        if (!symbol || !interval || !days) return;

        fetch(`/api/check_cache?symbol=${encodeURIComponent(symbol)}&interval=${encodeURIComponent(interval)}&days=${encodeURIComponent(days)}`)
            .then(res => res.json())
            .then(data => {
                if (data.cached) {
                    cacheIndicator.textContent = `✓ Ada di disk cache (${data.size_kb} KB) - siap kalkulasi instan.`;
                    cacheIndicator.style.color = 'var(--good-green)';
                } else {
                    cacheIndicator.textContent = `ℹ Belum ada cache - akan di-fetch dari Binance FAPI.`;
                    cacheIndicator.style.color = 'var(--highlight-gold)';
                }
            })
            .catch(() => {
                cacheIndicator.textContent = ``;
            });
    }

    document.getElementById('symbol').addEventListener('input', checkCache);
    document.getElementById('interval').addEventListener('change', checkCache);
    document.getElementById('days').addEventListener('input', checkCache);
    checkCache();

    // Tab buttons
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
            document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
            btn.classList.add('active');
            const target = document.getElementById(btn.dataset.tab);
            if (target) target.classList.add('active');
        });
    });

    // Helper: Form data to query string
    function getFormData() {
        const formData = new FormData(form);
        const params = new URLSearchParams();
        for (const [key, val] of formData.entries()) {
            params.append(key, val);
        }
        // Handle unchecked checkboxes
        const checkFields = ['force_fetch', 'realtimeUpdate', 'incTrend', 'incCounter', 'incHedge', 'incBarTp', 'incBarSl'];
        checkFields.forEach(chk => {
            const el = document.getElementById(chk);
            if (el && !el.checked) {
                params.set(chk, 'false');
            } else if (el && el.checked) {
                params.set(chk, 'true');
            }
        });
        return params;
    }

    // Helper: Form data to JSON object
    function getFormJSON() {
        const params = getFormData();
        const obj = {};
        for (const [key, val] of params.entries()) {
            const boolFields = ['force_fetch', 'realtimeUpdate', 'incTrend', 'incCounter', 'incHedge', 'incBarTp', 'incBarSl'];
            const intFields = ['days', 'volatilityLength', 'leverage', 'maxOpen', 'btDays', 'barTp1', 'barTp2', 'barTpDir', 'slBar1', 'slBar2', 'slBarAdv', 'minLevels', 'topN'];
            const floatFields = ['volMult', 'initCap', 'marginPct', 'feeEntry', 'feeExit', 'mmrPct', 'fixed1', 'fixed2', 'fixed3'];

            if (boolFields.includes(key)) {
                obj[key] = (val === 'true');
            } else if (intFields.includes(key)) {
                obj[key] = parseInt(val, 10);
            } else if (floatFields.includes(key)) {
                obj[key] = parseFloat(val);
            } else {
                obj[key] = val;
            }
        }
        return obj;
    }

    // Run Backtest via SSE
    btnRun.addEventListener('click', () => {
        if (currentEventSource) {
            currentEventSource.close();
            currentEventSource = null;
        }

        const params = getFormData();
        const url = `/api/stream_backtest?${params.toString()}`;

        btnRun.disabled = true;
        btnRecalc.disabled = true;
        progressContainer.style.display = 'block';
        milestoneAlert.style.display = 'none';
        progressLabel.textContent = 'Menghubungkan ke Binance Futures API...';
        progressPercent.textContent = '0%';
        progressBar.style.width = '0%';

        currentEventSource = new EventSource(url);

        currentEventSource.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);
                if (msg.type === 'progress') {
                    const info = msg.data;
                    progressLabel.textContent = info.message || 'Mengambil data...';
                    if (info.percent !== undefined) {
                        progressPercent.textContent = `${info.percent}%`;
                        progressBar.style.width = `${info.percent}%`;
                    }
                    if (info.status === 'milestone_pause') {
                        milestoneAlert.style.display = 'flex';
                        milestoneText.textContent = info.message;
                    } else {
                        milestoneAlert.style.display = 'none';
                    }
                } else if (msg.type === 'complete') {
                    currentEventSource.close();
                    currentEventSource = null;
                    btnRun.disabled = false;
                    btnRecalc.disabled = false;
                    progressContainer.style.display = 'none';

                    lastResult = msg.result;
                    renderResults(lastResult);
                    checkCache();
                } else if (msg.type === 'error') {
                    currentEventSource.close();
                    currentEventSource = null;
                    btnRun.disabled = false;
                    btnRecalc.disabled = false;
                    progressLabel.textContent = `Error: ${msg.message}`;
                    progressBar.style.backgroundColor = 'var(--bad-red)';
                    alert(`Gagal menjalankan simulasi: ${msg.message}`);
                }
            } catch (err) {
                console.error('SSE JSON error:', err);
            }
        };

        currentEventSource.onerror = (err) => {
            console.error('EventSource error:', err);
            if (currentEventSource) {
                currentEventSource.close();
                currentEventSource = null;
            }
            btnRun.disabled = false;
            btnRecalc.disabled = false;
            progressLabel.textContent = 'Koneksi terputus.';
        };
    });

    // Fast Recalculation (RAM/Cache)
    btnRecalc.addEventListener('click', () => {
        const payload = getFormJSON();
        btnRecalc.disabled = true;
        btnRun.disabled = true;

        progressContainer.style.display = 'block';
        milestoneAlert.style.display = 'none';
        progressLabel.textContent = 'Menghitung ulang dari RAM/Cache...';
        progressPercent.textContent = '50%';
        progressBar.style.width = '50%';

        fetch('/api/recalculate', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        })
        .then(res => res.json())
        .then(data => {
            btnRecalc.disabled = false;
            btnRun.disabled = false;
            progressContainer.style.display = 'none';

            if (data.error) {
                alert(data.error);
                return;
            }

            lastResult = data.result;
            renderResults(lastResult);
        })
        .catch(err => {
            btnRecalc.disabled = false;
            btnRun.disabled = false;
            progressContainer.style.display = 'none';
            alert(`Error hitung ulang: ${err.message}`);
        });
    });

    // Dynamic triggers: dropdown change -> fast recalculate
    document.getElementById('selStrategy').addEventListener('change', () => {
        if (lastResult) btnRecalc.click();
    });

    document.getElementById('stratFilter').addEventListener('change', () => {
        if (lastResult) btnRecalc.click();
    });

    document.getElementById('rankBy').addEventListener('change', () => {
        if (lastResult) btnRecalc.click();
    });

    document.getElementById('oneWayRule').addEventListener('change', () => {
        if (lastResult) btnRecalc.click();
    });

    document.getElementById('tableMode').addEventListener('change', () => {
        updateTableVisibility();
    });

    function updateTableVisibility() {
        const mode = document.getElementById('tableMode').value;
        const ldContainer = document.getElementById('leaderboard-container');
        const dtContainer = document.getElementById('detail-container');

        if (mode === 'Keduanya (Top + Detail)') {
            ldContainer.style.display = 'block';
            dtContainer.style.display = 'block';
            document.querySelector('.tables-grid').style.gridTemplateColumns = '1.5fr 1fr';
        } else if (mode === 'Top Strategy (Leaderboard Saja)') {
            ldContainer.style.display = 'block';
            dtContainer.style.display = 'none';
            document.querySelector('.tables-grid').style.gridTemplateColumns = '1fr';
        } else if (mode === 'Detail Strategy (Detail Saja)') {
            ldContainer.style.display = 'none';
            dtContainer.style.display = 'block';
            document.querySelector('.tables-grid').style.gridTemplateColumns = '1fr';
        }
    }

    // Render Full Results
    function renderResults(result) {
        if (!result) return;

        // 1. Summary Bar
        const summary = result.summary;
        const symbol = document.getElementById('symbol').value.trim().toUpperCase();
        const interval = document.getElementById('interval').value;
        sumCoin.textContent = `${symbol} [${interval}]`;
        sumCandles.textContent = `${summary.candles_count} (${summary.start_date.split(' ')[0]} s/d ${summary.end_date.split(' ')[0]})`;
        sumPrice.textContent = `${summary.last_price.toFixed(2)} USDT`;
        sumStrats.textContent = `${summary.filtered_sims} / ${summary.total_sims} Strategi`;
        summaryBar.style.display = 'flex';
        dataStatus.textContent = `${symbol} (${summary.candles_count} candles)`;
        dataStatus.className = 'badge badge-active';

        // 2. Leaderboard Table
        const ld = result.leaderboard;
        ldTitle.textContent = ld.title;
        ldBody.innerHTML = '';

        // Render Top N rows
        ld.rows.forEach(r => {
            const tr = document.createElement('tr');
            if (r.is_selected) {
                tr.classList.add('selected-row');
            }

            // Click to select row & update detail table
            tr.addEventListener('click', () => {
                document.getElementById('selStrategy').value = r.key;
                btnRecalc.click();
            });

            const netClass = (r.net_pct >= 0) ? 'val-good' : 'val-bad';
            const ddEqClass = (r.dd_eq > 0) ? 'val-bad' : 'val-neutral';
            const ddBalClass = (r.dd_bal > 0) ? 'val-bad' : 'val-neutral';
            const pfClass = (r.pf >= 1.0) ? 'val-good' : 'val-bad';
            const liqClass = (r.liq === 'YA') ? 'val-bad' : 'val-dim';

            let nameColClass = 'val-neutral';
            if (r.is_selected) nameColClass = 'val-highlight font-bold';
            else if (r.badge === 'bar2') nameColClass = 'val-bar2';
            else if (r.badge === 'btp') nameColClass = 'val-btp';
            else if (r.badge === 'bsl') nameColClass = 'val-bsl';
            else if (r.badge === 'fix') nameColClass = 'val-fix';
            else if (r.badge === 'med') nameColClass = 'val-med';

            tr.innerHTML = `
                <td class="text-center ${r.is_selected ? 'val-highlight font-bold' : nameColClass}">${r.rank}</td>
                <td class="text-left ${nameColClass}">${r.name}</td>
                <td class="text-right ${netClass}">${r.net_pct_str}</td>
                <td class="text-right ${ddEqClass}">${r.dd_eq_str}</td>
                <td class="text-right ${ddBalClass}">${r.dd_bal_str}</td>
                <td class="text-right val-neutral">${r.win_pct}</td>
                <td class="text-right val-neutral">${r.trades}</td>
                <td class="text-right ${pfClass}">${r.pf_str}</td>
                <td class="text-center ${liqClass}">${r.liq}</td>
            `;
            ldBody.appendChild(tr);
        });

        // Render 6 Category Winners comparison rows
        if (ld.category_winners && ld.category_winners.length > 0) {
            // Header Row
            const sumHeadTr = document.createElement('tr');
            sumHeadTr.className = 'cat-summary-header';
            sumHeadTr.innerHTML = `
                <td colspan="9" class="text-center">
                    🏆 PEMENANG TERBAIK: BASE BAR (TP+SL / TP / SL) vs BASE PERSEN (FIXED % / MEDIAN %)
                </td>
            `;
            ldBody.appendChild(sumHeadTr);

            // Winner Rows
            ld.category_winners.forEach(w => {
                const wTr = document.createElement('tr');
                wTr.className = 'cat-winner-row';

                wTr.addEventListener('click', () => {
                    document.getElementById('selStrategy').value = w.key;
                    btnRecalc.click();
                });

                const netClass = (w.net_pct >= 0) ? 'val-good' : 'val-bad';
                const ddEqClass = (w.dd_eq > 0) ? 'val-bad' : 'val-neutral';
                const ddBalClass = (w.dd_bal > 0) ? 'val-bad' : 'val-neutral';
                const pfClass = (w.pf >= 1.0) ? 'val-good' : 'val-bad';
                const liqClass = (w.liq === 'YA') ? 'val-bad' : 'val-dim';
                const colClass = `val-${w.color_code}`;

                wTr.innerHTML = `
                    <td class="text-center ${colClass} font-bold">${w.category_code}</td>
                    <td class="text-left ${colClass}"><span class="cat-badge cat-badge-${w.color_code}">${w.label}</span> ${w.name}</td>
                    <td class="text-right ${netClass}">${w.net_pct_str}</td>
                    <td class="text-right ${ddEqClass}">${w.dd_eq_str}</td>
                    <td class="text-right ${ddBalClass}">${w.dd_bal_str}</td>
                    <td class="text-right val-neutral">${w.win_pct}</td>
                    <td class="text-right val-neutral">${w.trades}</td>
                    <td class="text-right ${pfClass}">${w.pf_str}</td>
                    <td class="text-center ${liqClass}">${w.liq}</td>
                `;
                ldBody.appendChild(wTr);
            });
        }

        // 3. Detail Table
        const dt = result.detail;
        dtTitle.textContent = dt.title;
        dtBody.innerHTML = '';

        dt.rows.forEach(row => {
            const tr = document.createElement('tr');
            let valClass = 'val-neutral';
            if (row.color === 'good') valClass = 'val-good';
            else if (row.color === 'bad') valClass = 'val-bad';
            else if (row.color === 'highlight') valClass = 'val-highlight';

            tr.innerHTML = `
                <td>${row.key}</td>
                <td class="${valClass}">${row.val}</td>
            `;
            dtBody.appendChild(tr);
        });

        // 4. Aux Tables (Closed Trades & Open Positions)
        auxContainer.style.display = 'block';
        tradesBody.innerHTML = '';
        if (dt.recent_trades && dt.recent_trades.length > 0) {
            dt.recent_trades.forEach(t => {
                const tr = document.createElement('tr');
                const sideColor = (t.side === 'BUY') ? 'val-good' : 'val-bad';
                const pnlClass = (t.pnl_color === 'good') ? 'val-good' : 'val-bad';
                tr.innerHTML = `
                    <td class="${sideColor} font-bold">${t.side}</td>
                    <td>${t.entryTime}</td>
                    <td>${t.entryPx}</td>
                    <td>${t.tpDesc || t.tpPx}</td>
                    <td>${t.slDesc || t.slPx}</td>
                    <td>${t.exitTime}</td>
                    <td>${t.exitPx}</td>
                    <td class="val-highlight">${t.why}</td>
                    <td class="${pnlClass}">${t.movePct}</td>
                    <td class="${pnlClass}">${t.netPnl}</td>
                `;
                tradesBody.appendChild(tr);
            });
        } else {
            tradesBody.innerHTML = `<tr><td colspan="10" class="text-center empty-cell">Belum ada trade yang ditutup</td></tr>`;
        }

        positionsBody.innerHTML = '';
        if (dt.open_positions && dt.open_positions.length > 0) {
            dt.open_positions.forEach(p => {
                const tr = document.createElement('tr');
                const sideColor = (p.side === 'BUY') ? 'val-good' : 'val-bad';
                const pnlClass = (p.pnl_color === 'good') ? 'val-good' : 'val-bad';
                tr.innerHTML = `
                    <td class="${sideColor} font-bold">${p.side}</td>
                    <td>${p.entryTime}</td>
                    <td>${p.entryPx}</td>
                    <td>${p.qty}</td>
                    <td>${p.margin} USDT</td>
                    <td>${p.feeIn} USDT</td>
                    <td>${p.tpDesc || p.tpPx}</td>
                    <td>${p.slDesc || p.slPx}</td>
                    <td class="${pnlClass}">${p.floatPct}</td>
                    <td class="${pnlClass}">${p.floatUsdt}</td>
                `;
                positionsBody.appendChild(tr);
            });
        } else {
            positionsBody.innerHTML = `<tr><td colspan="10" class="text-center empty-cell">Tidak ada posisi yang sedang terbuka</td></tr>`;
        }

        updateTableVisibility();
    }
});
