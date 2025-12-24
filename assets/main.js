const handle = document.querySelector('.resize-handle');
const tree = document.querySelector('.file-tree');
let dragging = false;

handle.addEventListener('mousedown', (e) => {
    dragging = true;
    handle.classList.add('dragging');
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
});

document.addEventListener('mousemove', (e) => {
    if (!dragging) return;
    const width = e.clientX;
    if (width >= 150 && width <= 600) {
        tree.style.width = width + 'px';
        localStorage.setItem('pman-sidebar-width', width);
    }
});

document.addEventListener('mouseup', () => {
    dragging = false;
    handle.classList.remove('dragging');
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
});

// Restore saved width
const savedWidth = localStorage.getItem('pman-sidebar-width');
if (savedWidth) {
    tree.style.width = savedWidth + 'px';
}

// Directory collapse/expand
const expandedDirs = JSON.parse(localStorage.getItem('pman-expanded-dirs') || '[]');

function getDirPath(li) {
    const link = li.querySelector(':scope > a');
    return link ? link.getAttribute('href') : null;
}

function saveExpandedState() {
    const expanded = [];
    document.querySelectorAll('.file-tree li.dir:not(.collapsed)').forEach(li => {
        const path = getDirPath(li);
        if (path) expanded.push(path);
    });
    localStorage.setItem('pman-expanded-dirs', JSON.stringify(expanded));
}

// Start collapsed, restore expanded state
document.querySelectorAll('.file-tree li.dir').forEach(li => {
    const path = getDirPath(li);
    if (!path || !expandedDirs.includes(path)) {
        li.classList.add('collapsed');
    }
});

// Toggle on click (arrow or directory name)
document.querySelectorAll('.file-tree li.dir').forEach(li => {
    const toggle = li.querySelector(':scope > .toggle');
    const link = li.querySelector(':scope > a');

    function doToggle(e) {
        e.preventDefault();
        e.stopPropagation();
        li.classList.toggle('collapsed');
        saveExpandedState();
    }

    if (toggle) toggle.addEventListener('click', doToggle);
    if (link) link.addEventListener('click', doToggle);
});
