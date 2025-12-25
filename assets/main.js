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

// Highlight current file in tree and update path display
function highlightCurrentFile() {
    const currentPath = decodeURIComponent(location.pathname);

    // Update breadcrumb in navbar
    const pathDisplay = document.querySelector('.current-path');
    if (pathDisplay) {
        pathDisplay.innerHTML = '';
        const parts = currentPath.split('/').filter(p => p);

        // Add root link
        const rootLink = document.createElement('a');
        rootLink.href = '/';
        rootLink.textContent = 'Notes';
        rootLink.setAttribute('hx-get', '/');
        rootLink.setAttribute('hx-target', 'main');
        rootLink.setAttribute('hx-push-url', 'true');
        pathDisplay.appendChild(rootLink);

        // Add each path segment
        let href = '';
        parts.forEach((part, i) => {
            const sep = document.createElement('span');
            sep.textContent = ' / ';
            sep.className = 'breadcrumb-sep';
            pathDisplay.appendChild(sep);

            href += '/' + part;
            const link = document.createElement('a');
            link.href = href;
            link.textContent = part;
            link.setAttribute('hx-get', href);
            link.setAttribute('hx-target', 'main');
            link.setAttribute('hx-push-url', 'true');
            pathDisplay.appendChild(link);
        });

        // Re-process htmx on new elements
        if (window.htmx) {
            htmx.process(pathDisplay);
        }
    }

    // Remove previous active state
    document.querySelectorAll('.file-tree a.active').forEach(a => {
        a.classList.remove('active');
    });

    // Find and highlight current file
    let activeLink = null;
    document.querySelectorAll('.file-tree a').forEach(a => {
        const href = decodeURIComponent(a.getAttribute('href') || '');
        if (href === currentPath || href === currentPath + '/') {
            a.classList.add('active');
            activeLink = a;

            // Expand parent directories
            let parent = a.parentElement;
            while (parent) {
                if (parent.tagName === 'LI' && parent.classList.contains('dir')) {
                    parent.classList.add('expanded');
                }
                parent = parent.parentElement;
            }
        }
    });

    // Scroll after DOM updates
    if (activeLink) {
        requestAnimationFrame(() => {
            requestAnimationFrame(() => {
                activeLink.scrollIntoView({ behavior: 'smooth', block: 'center' });
            });
        });
    }
}

highlightCurrentFile();

// Re-highlight after htmx navigation
document.body.addEventListener('htmx:afterSettle', highlightCurrentFile);

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

// Live filtering of file tree
(function() {
    const searchInput = document.querySelector('.search-form input[name="q"]');
    if (!searchInput) return;

    function getExpandedDirs() {
        return JSON.parse(localStorage.getItem('para-expanded-dirs') || '[]');
    }

    function filterTree(query) {
        const lowerQuery = query.toLowerCase().trim();
        const allItems = document.querySelectorAll('.file-tree li');

        if (!lowerQuery) {
            // Reset: show all, restore saved expanded state and original text
            const expanded = getExpandedDirs();
            allItems.forEach(li => {
                li.style.display = '';
                li.classList.remove('filter-match', 'filter-expanded');
                const link = li.querySelector(':scope > a');
                if (link && link.dataset.originalText) {
                    link.textContent = link.dataset.originalText;
                }
            });
            document.querySelectorAll('.file-tree li.dir').forEach(li => {
                const link = li.querySelector(':scope > a');
                const path = link ? link.getAttribute('href') : null;
                if (path && expanded.includes(path)) {
                    li.classList.add('expanded');
                } else {
                    li.classList.remove('expanded');
                }
            });
            return;
        }

        // First pass: hide all, clear filter classes, restore original text
        allItems.forEach(li => {
            li.style.display = 'none';
            li.classList.remove('filter-match', 'filter-expanded');
            const link = li.querySelector(':scope > a');
            if (link && link.dataset.originalText) {
                link.textContent = link.dataset.originalText;
            }
        });

        // Find matching items
        allItems.forEach(li => {
            const link = li.querySelector(':scope > a');
            if (!link) return;

            // Store original text if not already stored
            if (!link.dataset.originalText) {
                link.dataset.originalText = link.textContent;
            }

            const originalText = link.dataset.originalText;
            const text = originalText.toLowerCase();
            if (text.includes(lowerQuery)) {
                li.classList.add('filter-match');
                li.style.display = '';

                // Highlight matching text
                const idx = text.indexOf(lowerQuery);
                const before = originalText.slice(0, idx);
                const match = originalText.slice(idx, idx + lowerQuery.length);
                const after = originalText.slice(idx + lowerQuery.length);
                link.innerHTML = `${before}<mark>${match}</mark>${after}`;

                // Show all parent directories
                let parent = li.parentElement;
                while (parent) {
                    if (parent.tagName === 'LI' && parent.classList.contains('dir')) {
                        parent.style.display = '';
                        parent.classList.add('expanded', 'filter-expanded');
                    }
                    parent = parent.parentElement;
                }
            }
        });

    }

    searchInput.addEventListener('input', (e) => {
        filterTree(e.target.value);
    });

    // Clear input after search is submitted
    const searchForm = document.querySelector('.search-form');
    if (searchForm) {
        searchForm.addEventListener('htmx:afterRequest', () => {
            searchInput.value = '';
            filterTree('');
        });
        searchForm.addEventListener('submit', () => {
            setTimeout(() => {
                searchInput.value = '';
                filterTree('');
            }, 0);
        });
    }
})();
