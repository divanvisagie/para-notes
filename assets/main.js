// Sidebar resize
const handle = document.querySelector('.resize-handle');
const sidebar = document.querySelector('.sidebar');

handle.addEventListener('mousedown', () => {
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    handle.classList.add('dragging');

    const onMouseMove = (e) => {
        const width = e.clientX;
        if (width >= 150 && width <= 600) {
            document.documentElement.style.setProperty('--sidebar-width', width + 'px');
            localStorage.setItem('para-sidebar-width', width);
        }
    };

    const onMouseUp = () => {
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
        handle.classList.remove('dragging');
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);
    };

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
});

// Directory toggle
document.querySelectorAll('.file-tree li.dir').forEach(li => {
    const toggle = li.querySelector(':scope > .toggle');
    const link = li.querySelector(':scope > a');

    const doToggle = (e) => {
        e.preventDefault();
        e.stopPropagation();
        li.classList.toggle('expanded');

        const expanded = [];
        document.querySelectorAll('.file-tree li.dir.expanded').forEach(el => {
            const a = el.querySelector(':scope > a');
            if (a) expanded.push(a.getAttribute('href'));
        });
        localStorage.setItem('para-expanded-dirs', JSON.stringify(expanded));
    };

    if (toggle) toggle.addEventListener('click', doToggle);
    if (link) link.addEventListener('click', doToggle);
});

// Copy buttons for code blocks
function addCopyButtons(container) {
    container.querySelectorAll('pre:not(.has-copy-btn)').forEach(pre => {
        pre.classList.add('has-copy-btn');
        const button = document.createElement('button');
        button.className = 'copy-button';
        button.textContent = 'Copy';
        button.addEventListener('click', () => {
            const code = pre.querySelector('code');
            const text = code ? code.textContent : pre.textContent;
            navigator.clipboard.writeText(text).then(() => {
                button.textContent = 'Copied!';
                setTimeout(() => { button.textContent = 'Copy'; }, 2000);
            });
        });
        pre.appendChild(button);
    });
}

addCopyButtons(document);

// Mermaid diagrams
mermaid.initialize({ startOnLoad: false, theme: 'neutral' });

async function renderMermaid(container) {
    const nodes = [];
    container.querySelectorAll('pre > code.language-mermaid').forEach(code => {
        const pre = code.parentElement;
        const div = document.createElement('div');
        div.className = 'mermaid';
        div.textContent = code.textContent;
        pre.replaceWith(div);
        nodes.push(div);
    });
    if (nodes.length > 0) {
        await mermaid.run({ nodes });
    }
}

renderMermaid(document);

// Re-add copy buttons and render mermaid after htmx swaps content
document.body.addEventListener('htmx:afterSwap', (e) => {
    addCopyButtons(e.detail.target);
    renderMermaid(e.detail.target);
});

// Live reload
(function() {
    let reconnectDelay = 1000;

    function connect() {
        const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
        const ws = new WebSocket(`${protocol}//${location.host}/ws`);

        ws.onopen = () => {
            console.log('[para] Live reload connected');
            reconnectDelay = 1000;
        };

        ws.onmessage = (e) => {
            try {
                const data = JSON.parse(e.data);
                if (data.type === 'reload') {
                    console.log('[para] Reloading:', data.path);
                    location.reload();
                }
            } catch (err) {
                console.error('[para] Parse error:', err);
            }
        };

        ws.onclose = () => {
            setTimeout(connect, reconnectDelay);
            reconnectDelay = Math.min(reconnectDelay * 2, 30000);
        };

        ws.onerror = () => ws.close();
    }

    connect();
})();
