use std::collections::BTreeMap;

/// Known modifier tags for specific modifier keys.
/// `*` means any tag string is accepted (e.g. `__delay.500ms`).
pub const ANY_TAG: &[&str] = &[];

pub struct ModifierDef {
    pub key: &'static str,
    pub description: &'static str,
    /// Known tag values. Empty slice = any value accepted.
    pub tags: &'static [&'static str],
}

pub fn all() -> BTreeMap<&'static str, ModifierDef> {
    [
        // ── Casing ──
        mk(
            "case",
            "Converts casing of the signal/event name.",
            &["camel", "kebab", "snake", "pascal"],
        ),
        // ── Timing ──
        mk("delay", "Delays event listener execution.", ANY_TAG), // e.g. .500ms, .1s
        mk("debounce", "Debounces the event listener.", ANY_TAG),
        mk("throttle", "Throttles the event listener.", ANY_TAG),
        mk("duration", "Sets interval duration.", ANY_TAG), // data-on-interval
        // ── Event modifiers ──
        mk("once", "Only triggers the event listener once.", &[]),
        mk("passive", "Does not call preventDefault on the event.", &[]),
        mk("capture", "Uses a capture event listener.", &[]),
        mk(
            "window",
            "Attaches the event listener to the window element.",
            &[],
        ),
        mk(
            "document",
            "Attaches the event listener to the document element.",
            &[],
        ),
        mk(
            "outside",
            "Triggers when the event is outside the element.",
            &[],
        ),
        mk("prevent", "Calls preventDefault on the event.", &[]),
        mk("stop", "Calls stopPropagation on the event.", &[]),
        // ── Bind-specific ──
        mk("prop", "Binds to a specific element property.", ANY_TAG),
        mk(
            "event",
            "Defines which events sync the element property back.",
            ANY_TAG,
        ),
        // ── Signals-specific ──
        mk(
            "ifmissing",
            "Only patches signal if it does not already exist.",
            &[],
        ),
        // ── Intersection ──
        mk(
            "exit",
            "Only triggers when the element exits the viewport.",
            &[],
        ),
        mk("half", "Triggers when half the element is visible.", &[]),
        mk("full", "Triggers when the full element is visible.", &[]),
        mk(
            "threshold",
            "Triggers at a specific visibility percentage.",
            ANY_TAG,
        ), // .25, .75
        // ── Visual ──
        mk(
            "viewtransition",
            "Wraps expression in startViewTransition().",
            &[],
        ),
        // ── Scroll ──
        mk("smooth", "Smooth scrolling animation.", &[]),
        mk("instant", "Instant scrolling.", &[]),
        mk("auto", "Auto scroll behavior (computed CSS).", &[]),
        mk("hstart", "Scrolls to horizontal start.", &[]),
        mk("hcenter", "Scrolls to horizontal center.", &[]),
        mk("hend", "Scrolls to horizontal end.", &[]),
        mk("hnearest", "Scrolls to nearest horizontal edge.", &[]),
        mk("vstart", "Scrolls to vertical start.", &[]),
        mk("vcenter", "Scrolls to vertical center.", &[]),
        mk("vend", "Scrolls to vertical end.", &[]),
        mk("vnearest", "Scrolls to nearest vertical edge.", &[]),
        mk("focus", "Focuses the element after scrolling.", &[]),
        // ── Persist ──
        mk("session", "Persists signals in session storage.", &[]),
        // ── Query string ──
        mk("filter", "Filters out empty values from query string.", &[]),
        mk(
            "history",
            "Enables browser history for query string sync.",
            &[],
        ),
        // ── json-signals ──
        mk(
            "terse",
            "Outputs compact JSON without extra whitespace.",
            &[],
        ),
    ]
    .into_iter()
    .map(|m| (m.key, m))
    .collect()
}

const fn mk(
    key: &'static str,
    description: &'static str,
    tags: &'static [&'static str],
) -> ModifierDef {
    ModifierDef {
        key,
        description,
        tags,
    }
}
