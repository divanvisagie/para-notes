let Crepe;
try {
    const module = await import('https://esm.sh/@milkdown/crepe@7?bundle');
    Crepe = module.Crepe || module.default;
} catch (err) {
    console.error('Failed to load Milkdown:', err);
}

let crepe = null;
let isEditing = false;
let currentPath = null;

const editToggle = document.getElementById('edit-toggle');
const main = document.querySelector('main');
const editorContainer = document.getElementById('milkdown-editor');

// Get the edit path from the main element's data attribute
function getEditPath() {
    const main = document.querySelector('main');
    return main?.dataset.editPath || null;
}

// Update edit button visibility
function updateEditButton() {
    if (editToggle) {
        const editPath = getEditPath();
        if (editPath) {
            editToggle.style.display = 'block';
            editToggle.textContent = isEditing ? 'Save & Close' : 'Edit';
            editToggle.classList.toggle('editing', isEditing);
        } else {
            editToggle.style.display = 'none';
        }
    }
}

// Fetch raw markdown content
async function fetchRawContent(path) {
    const response = await fetch(`/raw${path}`);
    if (!response.ok) throw new Error('Failed to fetch content');
    return response.text();
}

// Clean up markdown output to match original formatting
function cleanMarkdown(md) {
    return md
        // Unescape [[]] brackets: \[\[text]] -> [[text]]
        .replace(/\\\[\\\[/g, '[[')
        .replace(/\\\]\\\]/g, ']]')
        // Remove angle brackets from URLs: <https://...> -> https://...
        .replace(/<(https?:\/\/[^>]+)>/g, '$1');
}

// Save content
async function saveContent() {
    if (!crepe || !currentPath) return false;

    const rawContent = crepe.getMarkdown();
    const content = cleanMarkdown(rawContent);
    const response = await fetch('/save', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: currentPath, content })
    });

    const result = await response.json();
    if (!result.success) {
        alert('Save failed: ' + (result.error || 'Unknown error'));
        return false;
    }
    return true;
}

// Enter edit mode
async function enterEditMode() {
    if (isEditing) return;

    if (!Crepe) {
        alert('Editor failed to load. Check browser console for details.');
        return;
    }

    currentPath = getEditPath();
    if (!currentPath) {
        alert('No editable file on this page.');
        return;
    }

    try {
        const content = await fetchRawContent(currentPath);

        // Clear previous editor
        if (crepe) {
            crepe.destroy();
        }
        editorContainer.innerHTML = '';

        // Create new editor
        const config = {
            root: editorContainer,
            defaultValue: content,
        };

        // Disable problematic features if available
        if (Crepe.Feature) {
            config.features = {
                [Crepe.Feature.ImageBlock]: false,
                [Crepe.Feature.BlockEdit]: false,
                [Crepe.Feature.Placeholder]: false,
            };
        }

        crepe = new Crepe(config);
        await crepe.create();

        isEditing = true;
        main.classList.add('editing');
        updateEditButton();
    } catch (err) {
        console.error('Failed to enter edit mode:', err);
        alert('Failed to load editor: ' + err.message);
    }
}

// Exit edit mode
async function exitEditMode(save = true) {
    if (!isEditing) return;

    if (save) {
        const saved = await saveContent();
        if (!saved) return; // Don't exit if save failed
    }

    isEditing = false;
    main.classList.remove('editing');
    updateEditButton();

    // Reload page to show updated content
    if (save) {
        location.reload();
    }
}

// Toggle edit mode
async function toggleEditMode() {
    if (isEditing) {
        await exitEditMode(true);
    } else {
        await enterEditMode();
    }
}

// Event listeners
if (editToggle) {
    editToggle.addEventListener('click', toggleEditMode);
}

// Keyboard shortcut: Cmd/Ctrl+S to save
document.addEventListener('keydown', async (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault();
        if (isEditing) {
            const saved = await saveContent();
            if (saved) {
                // Brief visual feedback
                editToggle.textContent = 'Saved!';
                setTimeout(() => {
                    editToggle.textContent = 'Save & Close';
                }, 1000);
            }
        }
    }

    // Escape to exit edit mode without saving
    if (e.key === 'Escape' && isEditing) {
        if (confirm('Discard changes?')) {
            isEditing = false;
            main.classList.remove('editing');
            updateEditButton();
            location.reload();
        }
    }
});

// Handle page navigation
document.body.addEventListener('htmx:afterSettle', () => {
    // Exit edit mode on navigation
    if (isEditing) {
        isEditing = false;
        main.classList.remove('editing');
    }
    updateEditButton();
});

// Initial setup
updateEditButton();
