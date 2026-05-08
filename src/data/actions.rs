use std::collections::BTreeMap;

/// Describes a known Datastar action (e.g. `@get`, `@post`).
pub struct ActionDef {
    pub name: &'static str,
    pub description: &'static str,
    pub doc_url: &'static str,
    pub pro: bool,
    /// Parameter names (without types)
    pub params: &'static [&'static str],
}

pub fn all() -> BTreeMap<&'static str, ActionDef> {
    [
        // ── Free actions ──
        action(
            "get",
            "Sends a GET request to the backend and processes SSE events.",
            "https://data-star.dev/reference/actions#get",
            false,
            &["uri", "options"],
        ),
        action(
            "post",
            "Sends a POST request to the backend.",
            "https://data-star.dev/reference/actions#post",
            false,
            &["uri", "options"],
        ),
        action(
            "put",
            "Sends a PUT request to the backend.",
            "https://data-star.dev/reference/actions#put",
            false,
            &["uri", "options"],
        ),
        action(
            "patch",
            "Sends a PATCH request to the backend.",
            "https://data-star.dev/reference/actions#patch",
            false,
            &["uri", "options"],
        ),
        action(
            "delete",
            "Sends a DELETE request to the backend.",
            "https://data-star.dev/reference/actions#delete",
            false,
            &["uri", "options"],
        ),
        action(
            "peek",
            "Accesses signals without subscribing to changes.",
            "https://data-star.dev/reference/actions#peek",
            false,
            &["callable"],
        ),
        action(
            "setAll",
            "Sets the value of all matching signals.",
            "https://data-star.dev/reference/actions#setall",
            false,
            &["value", "filter?"],
        ),
        action(
            "toggleAll",
            "Toggles the boolean value of all matching signals.",
            "https://data-star.dev/reference/actions#toggleall",
            false,
            &["filter?"],
        ),
        // ── Pro actions ──
        action(
            "clipboard",
            "Copies text to the system clipboard.",
            "https://data-star.dev/reference/actions#clipboard",
            true,
            &["text", "isBase64?"],
        ),
        action(
            "fit",
            "Linearly interpolates a value from one range to another.",
            "https://data-star.dev/reference/actions#fit",
            true,
            &[
                "v",
                "oldMin",
                "oldMax",
                "newMin",
                "newMax",
                "shouldClamp?",
                "shouldRound?",
            ],
        ),
        action(
            "intl",
            "Provides locale-aware formatting for dates, numbers, etc.",
            "https://data-star.dev/reference/actions#intl",
            true,
            &["type", "value", "options?", "locale?"],
        ),
    ]
    .into_iter()
    .map(|a| (a.name, a))
    .collect()
}

const fn action(
    name: &'static str,
    description: &'static str,
    doc_url: &'static str,
    pro: bool,
    params: &'static [&'static str],
) -> ActionDef {
    ActionDef {
        name,
        description,
        doc_url,
        pro,
        params,
    }
}
