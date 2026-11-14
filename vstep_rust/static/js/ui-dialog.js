/**
 * VolStep UI Dialog & Toast System
 * Anti-slop clean notifications & confirmation modals without native browser popups
 */
(function () {
    // 1. Toast Container
    function getToastContainer() {
        let container = document.getElementById('ui-toast-container');
        if (!container) {
            container = document.createElement('div');
            container.id = 'ui-toast-container';
            container.className = 'ui-toast-container';
            document.body.appendChild(container);
        }
        return container;
    }

    // 2. Global Toast
    window.showToast = function (message, type = 'info', duration = 3500) {
        if (!message) return;
        const container = getToastContainer();

        const toast = document.createElement('div');
        toast.className = `ui-toast ui-toast-${type}`;

        const textSpan = document.createElement('span');
        textSpan.className = 'ui-toast-text';
        textSpan.textContent = message;

        const closeBtn = document.createElement('button');
        closeBtn.type = 'button';
        closeBtn.className = 'ui-toast-close';
        closeBtn.innerHTML = '&times;';
        closeBtn.onclick = () => removeToast(toast);

        toast.appendChild(textSpan);
        toast.appendChild(closeBtn);
        container.appendChild(toast);

        setTimeout(() => removeToast(toast), duration);
    };

    function removeToast(toast) {
        if (!toast || toast.dataset.removing) return;
        toast.dataset.removing = 'true';
        toast.classList.add('ui-toast-exit');
        setTimeout(() => {
            if (toast && toast.parentNode) {
                toast.parentNode.removeChild(toast);
            }
        }, 220);
    }

    // 3. Global Confirmation Modal
    window.showConfirm = function (options) {
        const {
            title = 'Konfirmasi',
            message = 'Apakah Anda yakin ingin melanjutkan?',
            confirmText = 'Ya, Lanjutkan',
            cancelText = 'Batal',
            onConfirm,
            onCancel,
            isDanger = false
        } = (typeof options === 'string' ? { message: options } : (options || {}));

        const overlay = document.createElement('div');
        overlay.className = 'ui-modal-overlay';

        overlay.innerHTML = `
            <div class="ui-modal-box">
                <div class="ui-modal-header">
                    <h3 class="ui-modal-title">${escapeHtml(title)}</h3>
                </div>
                <div class="ui-modal-body">
                    <p class="ui-modal-message">${escapeHtml(message)}</p>
                </div>
                <div class="ui-modal-footer">
                    <button type="button" class="ui-modal-btn ui-modal-cancel">${escapeHtml(cancelText)}</button>
                    <button type="button" class="ui-modal-btn ${isDanger ? 'ui-modal-danger' : 'ui-modal-confirm'}">${escapeHtml(confirmText)}</button>
                </div>
            </div>
        `;

        document.body.appendChild(overlay);

        const btnCancel = overlay.querySelector('.ui-modal-cancel');
        const btnConfirm = overlay.querySelector('.ui-modal-confirm, .ui-modal-danger');

        function closeModal() {
            overlay.classList.add('ui-modal-closing');
            setTimeout(() => {
                if (overlay && overlay.parentNode) {
                    overlay.parentNode.removeChild(overlay);
                }
            }, 180);
        }

        btnCancel.addEventListener('click', () => {
            closeModal();
            if (onCancel) onCancel();
        });

        btnConfirm.addEventListener('click', () => {
            closeModal();
            if (onConfirm) onConfirm();
        });

        overlay.addEventListener('click', (e) => {
            if (e.target === overlay) {
                closeModal();
                if (onCancel) onCancel();
            }
        });
    };

    // Override window.alert so native "127.0.0.1 says" popups never appear
    window.alert = function (msg) {
        const cleanMsg = String(msg || '');
        let type = 'info';
        if (cleanMsg.includes('✓') || cleanMsg.toLowerCase().includes('berhasil')) {
            type = 'success';
        } else if (
            cleanMsg.toLowerCase().includes('gagal') ||
            cleanMsg.toLowerCase().includes('error') ||
            cleanMsg.includes('❌') ||
            cleanMsg.includes('⚠️')
        ) {
            type = 'error';
        }
        window.showToast(cleanMsg, type);
    };

    function escapeHtml(str) {
        if (!str) return '';
        const div = document.createElement('div');
        div.textContent = str;
        return div.innerHTML;
    }
})();
