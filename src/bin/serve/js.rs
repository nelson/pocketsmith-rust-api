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
    // full re-render. block:'nearest' is a no-op when the active row
    // is already in the visible area, so a 'scroll only if needed'
    // policy falls out for free — *provided* the queue panel's
    // scrollTop is the same as it was before the swap. The body swap
    // resets it to 0, so we capture/restore in the htmx:beforeSwap /
    // htmx:afterSwap listeners below.
    const sel = document.querySelector('.queue-item.selected');
    if (sel) sel.scrollIntoView({block: 'nearest'});
}

// Captured between beforeSwap and afterSwap so a body innerHTML swap
// (the only kind that destroys #queue) doesn't drop the user's scroll
// position. Null means 'no .queue-list existed at capture time' — e.g.
// initial page load.
//
// Note: the scrollable element is .queue-list (overflow-y: auto), NOT
// #queue (overflow: hidden). #queue is the queue-panel container; its
// scrollTop is always 0 because nothing scrolls there.
let _savedQueueScrollTop = null;

// Fire on initial render. The script tag is at the end of body so
// document is fully parsed at this point.
scrollSelectedIntoView();

document.addEventListener('htmx:beforeSwap', function() {
    const list = document.querySelector('.queue-list');
    _savedQueueScrollTop = list ? list.scrollTop : null;
});

// Fire after every HTMX swap. Listening on document (not document.body)
// so the listener survives a body innerHTML swap.
document.addEventListener('htmx:afterSwap', function() {
    // Restore the queue's scroll position first. If the swap was
    // body-targeted, .queue-list is a brand-new element with
    // scrollTop=0; restoring puts the user back where they were. If
    // the swap was detail-targeted, .queue-list wasn't replaced and
    // this assignment is a harmless self-set.
    if (_savedQueueScrollTop !== null) {
        const list = document.querySelector('.queue-list');
        if (list) list.scrollTop = _savedQueueScrollTop;
    }
    // Now scroll the active row into view *only if it isn't already*
    // (block:'nearest' semantics). With the scroll position restored,
    // an active row that was visible before the action will still be
    // visible, and this call is a no-op. An active row that moved
    // off-screen (e.g. user pressed J past the viewport) gets the
    // minimal scroll to make it visible.
    scrollSelectedIntoView();
});

function getSelectedIndex() {
    const items = document.querySelectorAll('.queue-item');
    const selected = document.querySelector('.queue-item.selected');
    return Array.from(items).indexOf(selected);
}

document.addEventListener('click', function(e) {
    const item = e.target.closest('.queue-item');
    if (item) selectItem(item);
});

function navigateQueue(delta) {
    const items = document.querySelectorAll('.queue-item');
    if (items.length === 0) return;
    let idx = getSelectedIndex();
    if (idx === -1) idx = 0;
    idx = Math.max(0, Math.min(items.length - 1, idx + delta));
    const item = items[idx];
    selectItem(item);
    const url = item.dataset.detailUrl;
    const target = item.dataset.detailTarget || '#detail';
    if (url) htmx.ajax('GET', url, {target: target, swap: 'innerHTML'});
}

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
        navigateQueue(e.key === 'ArrowDown' ? 1 : -1);
        return;
    }

    // [ and ] step months on the dashboard. Bound globally for the
    // dashboard tab only (other tabs ignore them, which keeps these
    // characters typeable in any future text input we add).
    if ((e.key === '[' || e.key === ']') && document.body.classList.contains('tab-dashboard')) {
        e.preventDefault();
        navigateQueue(e.key === ']' ? -1 : 1); // ] = newer (up), [ = older (down)
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
        case '?':
            e.preventDefault();
            document.getElementById('hints-overlay').classList.toggle('open');
            break;
    }
    if (e.key === 'Escape') {
        const overlay = document.getElementById('hints-overlay');
        if (overlay && overlay.classList.contains('open')) {
            e.preventDefault();
            overlay.classList.remove('open');
        }
    }
});

} // end _navInitialized guard
"#;
