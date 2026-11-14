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
    const sumSr = document.getElementById('sum-sr');
    const sumVol = document.getElementById('sum-vol');
    const sumStrats = document.getElementById('sum-strats');

    const ldTitle = document.getElementById('ld-title');
    const ldBody = document.getElementById('ld-body');
    const ldCards = document.getElementById('ld-cards');
    const dtTitle = document.getElementById('dt-title');
    const dtBody = document.getElementById('dt-body');
    const dtCardsGrid = document.getElementById('dt-cards-grid');

    const catWinnersWrap = document.getElementById('cat-winners-wrap');
    const catWinnersGrid = document.getElementById('cat-winners-grid');

    const auxContainer = document.getElementById('aux-container');
    const tradesBody = document.getElementById('trades-body');
    const tradesCards = document.getElementById('trades-cards');
    const positionsBody = document.getElementById('positions-body');
    const positionsCards = document.getElementById('positions-cards');

    const dataStatus = document.getElementById('data-status');

    let currentEventSource = null;
    let lastResult = null;

    // Load 105 strategy keys into selStrategy
    fetch('/api/strategy_keys')
        .then(res => res.json())
        .then(data => {
            const keys = Array.isArray(data) ? data : (data.keys || []);
            const sel = document.getElementById('selStrategy');
            if (sel && keys.length > 0) {
                keys.forEach(k => {
                    const opt = document.createElement('option');
                    opt.value = k;
                    opt.textContent = k;
                    sel.appendChild(opt);
                });
            }
        })
        .catch(err => console.error('Gagal memuat strategy keys backtest:', err));

    // ==========================================
    // 1. MOBILE CHIPS INTERACTION
    // ==========================================
    // Coin Chips
    document.querySelectorAll('#coin-chips .chip').forEach(chip => {
        chip.addEventListener('click', () => {
            document.querySelectorAll('#coin-chips .chip').forEach(c => c.classList.remove('active'));
            chip.classList.add('active');
            document.getElementById('symbol').value = chip.dataset.sym;
            checkCache();
        });
    });

    // Timeframe Chips
    document.querySelectorAll('#tf-chips .chip').forEach(chip => {
        chip.addEventListener('click', () => {
            document.querySelectorAll('#tf-chips .chip').forEach(c => c.classList.remove('active'));
            chip.classList.add('active');
            document.getElementById('interval').value = chip.dataset.tf;
            checkCache();
        });
    });

    // Days Chips
    document.querySelectorAll('#days-chips .chip').forEach(chip => {
        chip.addEventListener('click', () => {
            document.querySelectorAll('#days-chips .chip').forEach(c => c.classList.remove('active'));
            chip.classList.add('active');
            document.getElementById('days').value = chip.dataset.days;
            checkCache();
        });
    });

    // Sync input typing to chips
    document.getElementById('symbol').addEventListener('input', () => {
        const val = document.getElementById('symbol').value.trim().toUpperCase();
        document.querySelectorAll('#coin-chips .chip').forEach(c => {
            c.classList.toggle('active', c.dataset.sym === val);
        });
        checkCache();
    });

    document.getElementById('interval').addEventListener('change', () => {
        const val = document.getElementById('interval').value;
        document.querySelectorAll('#tf-chips .chip').forEach(c => {
            c.classList.toggle('active', c.dataset.tf === val);
        });
        checkCache();
    });

    document.getElementById('days').addEventListener('input', () => {
        const val = document.getElementById('days').value;
        document.querySelectorAll('#days-chips .chip').forEach(c => {
            c.classList.toggle('active', c.dataset.days === val);
        });
        checkCache();
    });

    // ==========================================
    // 2. SEGMENTED MOBILE TABS
    // ==========================================
    const segTabs = document.querySelectorAll('.seg-tab');
    const segContents = document.querySelectorAll('.seg-content');

    function switchSegmentTab(targetId) {
        segTabs.forEach(t => t.classList.toggle('active', t.dataset.tab === targetId));
        segContents.forEach(c => {
            c.style.display = (c.id === targetId) ? 'block' : 'none';
        });
    }

    segTabs.forEach(tab => {
        tab.addEventListener('click', () => {
            switchSegmentTab(tab.dataset.tab);
        });
    });

    // Back to Leaderboard button in Detail view
    const btnBackToLd = document.getElementById('btn-back-to-ld');
    if (btnBackToLd) {
        btnBackToLd.addEventListener('click', () => {
            switchSegmentTab('tab-leaderboard');
        });
    }

    // View Mode Toggle (Kartu vs Tabel)
    const btnViewCards = document.getElementById('btn-view-cards');
    const btnViewTable = document.getElementById('btn-view-table');
    const ldTableContainer = document.getElementById('ld-table-container');

    if (btnViewCards && btnViewTable) {
        btnViewCards.addEventListener('click', () => {
            btnViewCards.classList.add('active');
            btnViewTable.classList.remove('active');
            if (ldCards) ldCards.style.display = 'flex';
            if (ldTableContainer) ldTableContainer.style.display = 'none';
        });

        btnViewTable.addEventListener('click', () => {
            btnViewTable.classList.add('active');
            btnViewCards.classList.remove('active');
            if (ldCards) ldCards.style.display = 'none';
            if (ldTableContainer) ldTableContainer.style.display = 'block';
        });
    }

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
                    cacheIndicator.textContent = `✓ Ada di disk cache (${data.size_kb} KB) - siap simulasi instan.`;
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

    checkCache();

    // Tab buttons for Aux Trades / Positions
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

    // Run Full Simulation with SSE Streaming
    btnRun.addEventListener('click', () => {
        const symbol = document.getElementById('symbol').value.trim().toUpperCase();
        if (!symbol) {
            alert('Silakan masukkan nama koin/symbol!');
            return;
        }

        if (currentEventSource) {
            currentEventSource.close();
        }

        btnRun.disabled = true;
        btnRecalc.disabled = true;
        progressContainer.style.display = 'block';
        milestoneAlert.style.display = 'none';
        progressLabel.textContent = 'Menghubungi Binance Futures API...';
        progressPercent.textContent = '0%';
        progressBar.style.width = '0%';
        progressBar.style.backgroundColor = 'var(--primary-blue)';

        // Switch to leaderboard tab
        switchSegmentTab('tab-leaderboard');

        const params = getFormData();
        const sseUrl = `/api/stream_backtest?${params.toString()}`;

        currentEventSource = new EventSource(sseUrl);

        currentEventSource.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);
                if (msg.type === 'progress') {
                    progressLabel.textContent = msg.message;
                    progressPercent.textContent = `${msg.percent.toFixed(1)}%`;
                    progressBar.style.width = `${msg.percent}%`;

                    if (msg.status === 'milestone_wait') {
                        milestoneAlert.style.display = 'flex';
                        milestoneText.textContent = `Batas kelipatan 360 hari tercapai. Jeda jeda API Binance tersisa ${msg.pause_left || 60}s...`;
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

    // ==========================================
    // 3. RENDER RESULTS (CARDS + TABLES)
    // ==========================================
    function renderResults(result) {
        if (!result) return;

        // 1. Summary Bar (Nilai persis tanpa dibulatkan)
        const summary = result.summary;
        const symbol = document.getElementById('symbol').value.trim().toUpperCase();
        const interval = document.getElementById('interval').value;
        sumCoin.textContent = `${symbol} [${interval}]`;
        sumCandles.textContent = `${summary.candles_count} (${summary.start_date.split(' ')[0]} s/d ${summary.end_date.split(' ')[0]})`;
        sumPrice.textContent = `${summary.last_price_str || summary.last_price} USDT`;
        if (sumSr) sumSr.textContent = `${summary.last_sr_str || summary.last_sr}`;
        if (sumVol) sumVol.textContent = `±${summary.last_vol_str || summary.last_vol}`;
        sumStrats.textContent = `${summary.filtered_sims} / ${summary.total_sims} Strategi`;
        summaryBar.style.display = 'flex';
        dataStatus.textContent = `${symbol} (${summary.candles_count} candles)`;
        dataStatus.className = 'badge badge-active';

        const ld = result.leaderboard;
        ldTitle.textContent = `${ld.title}`;

        // 2. Render Category Winners Carousel / Grid
        if (ld.category_winners && ld.category_winners.length > 0) {
            catWinnersWrap.style.display = 'block';
            catWinnersGrid.innerHTML = '';

            ld.category_winners.forEach(w => {
                const card = document.createElement('div');
                card.className = 'cat-winner-card';
                const netClass = (w.net_pct >= 0) ? 'val-good' : 'val-bad';

                card.innerHTML = `
                    <span class="cat-winner-tag cwt-${w.color_code}">${w.label}</span>
                    <div class="cat-winner-name" title="${w.name}">${w.name}</div>
                    <div class="cat-winner-profit ${netClass}">${w.net_pct_str}</div>
                    <div style="font-size: 10px; color: var(--text-dim); margin-top: 2px;">Win: ${w.win_pct} • PF: ${w.pf_str}</div>
                `;

                card.addEventListener('click', () => {
                    document.getElementById('selStrategy').value = w.key;
                    btnRecalc.click();
                    switchSegmentTab('tab-detail');
                });

                catWinnersGrid.appendChild(card);
            });
        }

        // 3. Render Mobile Strategy Cards (Default for Mobile)
        if (ldCards) {
            ldCards.innerHTML = '';
            ld.rows.forEach(r => {
                const card = document.createElement('div');
                card.className = `strat-mobile-card ${r.is_selected ? 'selected-card' : ''}`;

                let rankBadge = `#${r.rank}`;
                if (r.rank === 1) rankBadge = '🥇 #1';
                else if (r.rank === 2) rankBadge = '🥈 #2';
                else if (r.rank === 3) rankBadge = '🥉 #3';

                const netClass = (r.net_pct >= 0) ? 'val-good' : 'val-bad';
                const pfClass = (r.pf >= 1.0) ? 'val-good' : 'val-bad';
                const liqClass = (r.liq === 'YA') ? 'val-bad' : 'val-dim';

                card.innerHTML = `
                    <div class="smc-header">
                        <span class="smc-rank">${rankBadge}</span>
                        <span class="smc-title ${r.is_selected ? 'val-highlight font-bold' : ''}">${r.name}</span>
                        <span class="smc-win-badge">${r.win_pct} Win</span>
                    </div>
                    <div class="smc-grid">
                        <div class="smc-col">
                            <span class="smc-lbl">NET PROFIT</span>
                            <span class="smc-val large ${netClass}">${r.net_pct_str}</span>
                        </div>
                        <div class="smc-col">
                            <span class="smc-lbl">MAX DD (EQ/BAL)</span>
                            <span class="smc-val val-bad">${r.dd_eq_str} / ${r.dd_bal_str}</span>
                        </div>
                        <div class="smc-col">
                            <span class="smc-lbl">PROFIT FACTOR</span>
                            <span class="smc-val ${pfClass}">${r.pf_str}</span>
                        </div>
                    </div>
                    <div class="smc-footer">
                        <span>Trades: <strong style="color: #fff;">${r.trades}</strong> • Liq: <span class="${liqClass}">${r.liq}</span></span>
                        <span style="color: var(--primary-blue); font-weight: 700;">Ketuk untuk detail metrik ➔</span>
                    </div>
                `;

                card.addEventListener('click', () => {
                    document.getElementById('selStrategy').value = r.key;
                    btnRecalc.click();
                    switchSegmentTab('tab-detail');
                });

                ldCards.appendChild(card);
            });
        }

        // 4. Render Leaderboard Table (Spreadsheet View)
        ldBody.innerHTML = '';
        ld.rows.forEach(r => {
            const tr = document.createElement('tr');
            if (r.is_selected) tr.classList.add('selected-row');

            tr.addEventListener('click', () => {
                document.getElementById('selStrategy').value = r.key;
                btnRecalc.click();
                switchSegmentTab('tab-detail');
            });

            const netClass = (r.net_pct >= 0) ? 'val-good' : 'val-bad';
            const ddEqClass = (r.dd_eq > 0) ? 'val-bad' : 'val-neutral';
            const ddBalClass = (r.dd_bal > 0) ? 'val-bad' : 'val-neutral';
            const pfClass = (r.pf >= 1.0) ? 'val-good' : 'val-bad';
            const liqClass = (r.liq === 'YA') ? 'val-bad' : 'val-dim';

            tr.innerHTML = `
                <td class="text-center ${r.is_selected ? 'val-highlight font-bold' : ''}">${r.rank}</td>
                <td class="text-left ${r.is_selected ? 'val-highlight font-bold' : ''}">${r.name}</td>
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

        // 5. Render Detail Strategy (Cards + Table)
        const dt = result.detail;
        dtTitle.textContent = dt.title;

        // Render Metric Cards
        if (dtCardsGrid && dt.rows) {
            dtCardsGrid.innerHTML = '';

            // Group 1: Performa Bersih
            const perfRows = dt.rows.filter(r => r.key.includes('Net') || r.key.includes('Modal') || r.key.includes('Balance'));
            // Group 2: Risiko & Drawdown
            const riskRows = dt.rows.filter(r => r.key.includes('Drawdown') || r.key.includes('Margin') || r.key.includes('Liq'));
            // Group 3: Statistik Trade
            const tradeRows = dt.rows.filter(r => r.key.includes('Trade') || r.key.includes('Win Rate') || r.key.includes('Profit Factor'));
            // Group 4: Analisis Lainnya
            const otherRows = dt.rows.filter(r => !perfRows.includes(r) && !riskRows.includes(r) && !tradeRows.includes(r));

            const groups = [
                { title: '📈 Performa Modal & Profit', rows: perfRows },
                { title: '🛡️ Risiko & Drawdown', rows: riskRows },
                { title: '🎯 Statistik Win / Loss', rows: tradeRows },
                { title: '📊 Metrik Tambahan', rows: otherRows }
            ];

            groups.forEach(g => {
                if (g.rows.length === 0) return;
                const card = document.createElement('div');
                card.className = 'detail-metric-card';
                let rowsHtml = '';
                g.rows.forEach(row => {
                    let valClass = 'val-neutral';
                    if (row.color === 'good') valClass = 'val-good';
                    else if (row.color === 'bad') valClass = 'val-bad';
                    else if (row.color === 'highlight') valClass = 'val-highlight';

                    rowsHtml += `
                        <div class="dmc-row">
                            <span class="dmc-label">${row.key}</span>
                            <span class="dmc-value ${valClass}">${row.val}</span>
                        </div>
                    `;
                });

                card.innerHTML = `
                    <div class="dmc-title">${g.title}</div>
                    ${rowsHtml}
                `;
                dtCardsGrid.appendChild(card);
            });
        }

        // Render Detail Table Fallback
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

        // 6. Render Trades & Positions (Unrounded Precision)
        tradesBody.innerHTML = '';
        if (tradesCards) tradesCards.innerHTML = '';
        const tradesCount = document.getElementById('trades-count');
        if (dt.recent_trades && dt.recent_trades.length > 0) {
            if (tradesCount) tradesCount.textContent = dt.recent_trades.length;
            dt.recent_trades.forEach((t, idx) => {
                const trNum = idx + 1;
                const sideColor = (t.side === 'BUY') ? 'val-good' : 'val-bad';
                const pnlClass = (t.pnl_color === 'good') ? 'val-good' : 'val-bad';
                const entryPxVal = t.entryPxStr || t.entryPx;
                const exitPxVal = t.exitPxStr || t.exitPx;
                const closePxVal = (t.closePxStr !== undefined && t.closePxStr !== '') ? t.closePxStr : (t.closePx !== undefined ? t.closePx : '-');
                const srPxVal = (t.srPxStr !== undefined && t.srPxStr !== '') ? t.srPxStr : (t.srPx !== undefined ? t.srPx : '-');

                // Desktop Table Row (11 Kolom Lengkap & Presisi)
                const tr = document.createElement('tr');
                tr.innerHTML = `
                    <td class="font-bold text-dim">#${trNum}</td>
                    <td class="${sideColor} font-bold">${t.side}</td>
                    <td>${t.entryTime}</td>
                    <td class="font-mono">${entryPxVal}</td>
                    <td>${t.exitTime}</td>
                    <td class="font-mono">${exitPxVal}</td>
                    <td class="font-mono val-highlight">${closePxVal}</td>
                    <td class="font-mono" style="color: var(--accent-blue);">${srPxVal}</td>
                    <td><span class="badge badge-idle" style="font-size: 10px;">${t.why}</span></td>
                    <td class="${pnlClass}">${t.movePct}</td>
                    <td class="${pnlClass} font-bold">${t.netPnl}</td>
                `;
                tradesBody.appendChild(tr);

                // Mobile Card View
                if (tradesCards) {
                    const card = document.createElement('div');
                    card.style.background = '#151924';
                    card.style.border = '1px solid var(--border-color)';
                    card.style.borderRadius = '8px';
                    card.style.padding = '10px 12px';
                    card.style.fontSize = '12px';

                    card.innerHTML = `
                        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
                            <div>
                                <span style="font-weight: 700; color: var(--text-dim); margin-right: 6px;">#${trNum}</span>
                                <span class="badge ${t.side === 'BUY' ? 'badge-online' : 'badge-idle'}" style="font-weight: 700;">${t.side}</span>
                                <span style="margin-left: 6px; font-size: 11px; color: var(--text-dim);">${t.exitTime}</span>
                            </div>
                            <div class="${pnlClass}" style="font-weight: 700;">
                                ${t.netPnl} (${t.movePct})
                            </div>
                        </div>
                        <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 4px; font-size: 11px; background: rgba(255,255,255,0.02); padding: 6px; border-radius: 6px;">
                            <div><span style="color: var(--text-dim);">Entry Px:</span> <strong class="font-mono">${entryPxVal}</strong></div>
                            <div><span style="color: var(--text-dim);">Exit Px:</span> <strong class="font-mono">${exitPxVal}</strong></div>
                            <div><span style="color: var(--text-dim);">Close Px:</span> <strong class="font-mono val-highlight">${closePxVal}</strong></div>
                            <div><span style="color: var(--text-dim);">Level SR:</span> <strong class="font-mono" style="color: var(--accent-blue);">${srPxVal}</strong></div>
                        </div>
                        <div style="margin-top: 6px; font-size: 11px; color: var(--text-dim); display: flex; justify-content: space-between;">
                            <span>Alasan: <strong>${t.why}</strong></span>
                            <span>TP: ${t.tpDesc || t.tpPx} | SL: ${t.slDesc || t.slPx}</span>
                        </div>
                    `;
                    tradesCards.appendChild(card);
                }
            });
        } else {
            if (tradesCount) tradesCount.textContent = '0';
            tradesBody.innerHTML = `<tr><td colspan="11" class="text-center empty-cell">Belum ada trade yang ditutup</td></tr>`;
            if (tradesCards) tradesCards.innerHTML = `<div style="text-align: center; padding: 20px; color: var(--text-dim);">Belum ada trade yang ditutup</div>`;
        }

        positionsBody.innerHTML = '';
        if (positionsCards) positionsCards.innerHTML = '';
        const positionsCount = document.getElementById('positions-count');
        if (dt.open_positions && dt.open_positions.length > 0) {
            if (positionsCount) positionsCount.textContent = dt.open_positions.length;
            dt.open_positions.forEach((p, idx) => {
                const trNum = idx + 1;
                const sideColor = (p.side === 'BUY') ? 'val-good' : 'val-bad';
                const pnlClass = (p.pnl_color === 'good') ? 'val-good' : 'val-bad';
                const entryPxVal = p.entryPxStr || p.entryPx;
                const closePxVal = (p.currentClosePxStr !== undefined && p.currentClosePxStr !== '') ? p.currentClosePxStr : (p.currentClosePx !== undefined ? p.currentClosePx : '-');
                const srPxVal = (p.currentSrPxStr !== undefined && p.currentSrPxStr !== '') ? p.currentSrPxStr : (p.currentSrPx !== undefined ? p.currentSrPx : '-');

                // Desktop Table Row
                const tr = document.createElement('tr');
                tr.innerHTML = `
                    <td class="font-bold text-dim">#${trNum}</td>
                    <td class="${sideColor} font-bold">${p.side}</td>
                    <td>${p.entryTime}</td>
                    <td class="font-mono">${entryPxVal}</td>
                    <td class="font-mono val-highlight">${closePxVal}</td>
                    <td class="font-mono" style="color: var(--accent-blue);">${srPxVal}</td>
                    <td>${p.qty}</td>
                    <td>${p.margin} USDT</td>
                    <td>${p.tpDesc || p.tpPx}</td>
                    <td>${p.slDesc || p.slPx}</td>
                    <td class="${pnlClass} font-bold">${p.floatUsdt} (${p.floatPct})</td>
                `;
                positionsBody.appendChild(tr);

                // Mobile Card View
                if (positionsCards) {
                    const card = document.createElement('div');
                    card.style.background = '#151924';
                    card.style.border = '1px solid var(--border-color)';
                    card.style.borderRadius = '8px';
                    card.style.padding = '10px 12px';
                    card.style.fontSize = '12px';

                    card.innerHTML = `
                        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
                            <div>
                                <span style="font-weight: 700; color: var(--text-dim); margin-right: 6px;">#${trNum}</span>
                                <span class="badge ${p.side === 'BUY' ? 'badge-online' : 'badge-idle'}" style="font-weight: 700;">${p.side}</span>
                                <span style="margin-left: 6px; font-size: 11px; color: var(--text-dim);">${p.entryTime}</span>
                            </div>
                            <div class="${pnlClass}" style="font-weight: 700;">
                                ${p.floatUsdt} (${p.floatPct})
                            </div>
                        </div>
                        <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 4px; font-size: 11px; background: rgba(255,255,255,0.02); padding: 6px; border-radius: 6px;">
                            <div><span style="color: var(--text-dim);">Entry Px:</span> <strong class="font-mono">${entryPxVal}</strong></div>
                            <div><span style="color: var(--text-dim);">Margin:</span> <strong>${p.margin} USDT</strong></div>
                            <div><span style="color: var(--text-dim);">Close Px:</span> <strong class="font-mono val-highlight">${closePxVal}</strong></div>
                            <div><span style="color: var(--text-dim);">Level SR:</span> <strong class="font-mono" style="color: var(--accent-blue);">${srPxVal}</strong></div>
                        </div>
                    `;
                    positionsCards.appendChild(card);
                }
            });
        } else {
            if (positionsCount) positionsCount.textContent = '0';
            positionsBody.innerHTML = `<tr><td colspan="11" class="text-center empty-cell">Tidak ada posisi yang sedang terbuka</td></tr>`;
            if (positionsCards) positionsCards.innerHTML = `<div style="text-align: center; padding: 20px; color: var(--text-dim);">Tidak ada posisi yang sedang terbuka</div>`;
        }
    }
});
