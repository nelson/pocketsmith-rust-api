// Client-side JavaScript for keyboard navigation and HTMX-driven actions.
// Embedded as a string constant and injected into <script> by views::full_page.
// Uses a _navInitialized guard so the listener isn't duplicated on HTMX body swaps.
//
// Tab-agnostic via data attributes:
//
//   On each .queue-item:
//     data-detail-url     – URL to fetch for the detail panel on arrow nav.
//     data-detail-target  – CSS selector of the panel to swap into.
//
//   On each .actions container in the detail panel:
//     data-action-base    – URL prefix shared by confirm/reject/skip POSTs.
//                           e.g. "/transfers/pair/3-4" or "/normalise/item/abc".
//                           Y posts {base}/confirm, N posts {base}/reject,
//                           S posts {base}/skip.
//
// Event listeners:
//   click on .queue-item – calls selectItem to mark clicked row as active.
//   keydown:
//     ArrowUp/ArrowDown – navigates the queue list and triggers an htmx.ajax
//                         GET on data-detail-url into data-detail-target.
//     Y / N / S         – POST {data-action-base}/{confirm|reject|skip}.
//     U                 – clicks the .undo-btn if present.
pub const JS: &str = r#"
if (!window._navInitialized) {
window._navInitialized = true;

function selectItem(item) {
    document.querySelectorAll('.queue-item.selected').forEach(el => el.classList.remove('selected'));
    item.classList.add('selected');
    item.scrollIntoView({block: 'nearest'});
}

function scrollSelectedIntoView() {
    // Run on initial page load and after every HTMX body swap so the
    // user's place in the queue is preserved when an action causes a
    // full re-render. Without this the queue panel scrolls back to
    // its top after every Y/N/S because the panel element is replaced
    // (innerHTML swap on body) and the browser does not restore the
    // scroll position of internal scrollable containers.
    const sel = document.querySelector('.queue-item.selected');
    if (sel) sel.scrollIntoView({block: 'nearest'});
}

// Fire on initial render. The script tag is at the end of body so
// document is fully parsed at this point.
scrollSelectedIntoView();

// Fire after every HTMX swap (action POSTs target body, fragment GETs
// target #detail — in either case re-running this is cheap and
// idempotent). Listening on document (not document.body) so the
// listener survives a body innerHTML swap.
document.addEventListener('htmx:afterSwap', scrollSelectedIntoView);

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

    // Tab / Shift+Tab cycles between top-level tabs (Transfers / Normalise).
    if (e.key === 'Tab') {
        const tabs = Array.from(document.querySelectorAll('.tab-bar .tab'));
        if (tabs.length < 2) return;
        e.preventDefault();
        const activeIdx = tabs.findIndex(t => t.classList.contains('active'));
        const dir = e.shiftKey ? -1 : 1;
        const nextIdx = ((activeIdx === -1 ? 0 : activeIdx) + dir + tabs.length) % tabs.length;
        const next = tabs[nextIdx];
        // Inactive tabs are <a> with href; active is <span>. Navigate via href.
        const href = next.getAttribute('href');
        if (href) window.location.href = href;
        return;
    }

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
        const item = items[idx];
        selectItem(item);
        const url = item.dataset.detailUrl;
        const target = item.dataset.detailTarget || '#detail';
        if (url) htmx.ajax('GET', url, {target: target, swap: 'innerHTML'});
        return;
    }

    const actions = document.querySelector('.actions');
    const base = actions ? actions.dataset.actionBase : null;

    switch(e.key.toLowerCase()) {
        case 'y':
            if (!base) return;
            e.preventDefault();
            htmx.ajax('POST', base + '/confirm', {target: 'body'});
            break;
        case 'n':
            if (!base) return;
            e.preventDefault();
            htmx.ajax('POST', base + '/reject', {target: 'body'});
            break;
        case 's':
            if (!base) return;
            e.preventDefault();
            htmx.ajax('POST', base + '/skip', {target: 'body'});
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
