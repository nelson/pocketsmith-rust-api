pub const JS: &str = r#"
if (!window._navInitialized) {
window._navInitialized = true;

function selectItem(item) {
    document.querySelectorAll('.queue-item.selected').forEach(el => el.classList.remove('selected'));
    item.classList.add('selected');
    item.scrollIntoView({block: 'nearest'});
}

function getSelectedIndex() {
    const items = document.querySelectorAll('.queue-item');
    const selected = document.querySelector('.queue-item.selected');
    return Array.from(items).indexOf(selected);
}

document.addEventListener('click', function(e) {
    const item = e.target.closest('.queue-item');
    if (item) selectItem(item);
});

document.addEventListener('keydown', function(e) {
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;

    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const items = document.querySelectorAll('.queue-item');
        if (items.length === 0) return;
        let idx = getSelectedIndex();
        if (idx === -1) idx = 0;
        if (e.key === 'ArrowDown') {
            idx = Math.min(idx + 1, items.length - 1);
        } else {
            idx = Math.max(idx - 1, 0);
        }
        selectItem(items[idx]);
        htmx.ajax('GET', '/pair/' + items[idx].dataset.pairId, {target: '#detail', swap: 'innerHTML'});
        return;
    }

    const actions = document.querySelector('.actions');
    const pairId = actions ? actions.dataset.pairId : null;

    switch(e.key.toLowerCase()) {
        case 'y':
            if (!pairId) return;
            e.preventDefault();
            htmx.ajax('POST', '/pair/' + pairId + '/confirm', {target: 'body'});
            break;
        case 'n':
            if (!pairId) return;
            e.preventDefault();
            htmx.ajax('POST', '/pair/' + pairId + '/reject', {target: 'body'});
            break;
        case 's':
            if (!pairId) return;
            e.preventDefault();
            htmx.ajax('POST', '/pair/' + pairId + '/skip', {target: 'body'});
            break;
        case 'u':
            e.preventDefault();
            const undoBtn = document.querySelector('.undo-btn');
            if (undoBtn) undoBtn.click();
            break;
    }
});

} // end _navInitialized guard
"#;
