/// Curated real-world examples for each Datastar attribute, shown in hover documentation.
use std::collections::BTreeMap;

pub struct Example {
    pub title: &'static str,
    pub html: &'static str,
}

type ExampleMap = BTreeMap<&'static str, &'static [Example]>;

fn examples() -> ExampleMap {
    [
        (
            "on",
            &[
                Example {
                    title: "Click handler",
                    html: r#"<button data-on:click="@post('/api/submit')">Submit</button>"#,
                },
                Example {
                    title: "Debounced input",
                    html: r#"<input data-on:input__debounce.500ms="@get('/search?q=$query')" />"#,
                },
                Example {
                    title: "Keyboard shortcut",
                    html: r#"<div data-on:keydown__window="$shortcutHandler"></div>"#,
                },
                Example {
                    title: "Once-only load",
                    html: r#"<div data-on:load__once="@get('/api/init')"></div>"#,
                },
            ][..],
        ),
        (
            "signals",
            &[
                Example {
                    title: "Named signal",
                    html: r#"<div data-signals:count="0"></div>"#,
                },
                Example {
                    title: "Merged signals",
                    html: r#"<div data-signals="{firstName: '', lastName: '', age: 0}"></div>"#,
                },
                Example {
                    title: "Boolean signal",
                    html: r#"<div data-signals:isOpen="false"></div>"#,
                },
            ][..],
        ),
        (
            "text",
            &[
                Example {
                    title: "Display signal value",
                    html: r#"<span data-text="$userName"></span>"#,
                },
                Example {
                    title: "Computed text",
                    html: r#"<span data-text="$firstName + ' ' + $lastName"></span>"#,
                },
            ][..],
        ),
        (
            "show",
            &[
                Example {
                    title: "Conditional visibility",
                    html: r#"<div data-show="$isLoggedIn">Welcome back!</div>"#,
                },
                Example {
                    title: "Comparison",
                    html: r#"<div data-show="$count > 0">You have items</div>"#,
                },
            ][..],
        ),
        (
            "class",
            &[
                Example {
                    title: "Toggle CSS class",
                    html: r#"<div data-class:active="$isActive">Tab</div>"#,
                },
                Example {
                    title: "Error styling",
                    html: r#"<input data-class:error="$hasError" />"#,
                },
            ][..],
        ),
        (
            "ref",
            &[Example {
                title: "Element reference",
                html: r#"<canvas data-ref="myCanvas"></canvas>"#,
            }][..],
        ),
        (
            "bind",
            &[
                Example {
                    title: "Two-way input binding",
                    html: r#"<input data-bind:value="$username" />"#,
                },
                Example {
                    title: "Checkbox binding",
                    html: r#"<input type="checkbox" data-bind:checked="$isActive" />"#,
                },
            ][..],
        ),
        (
            "computed",
            &[Example {
                title: "Computed value",
                html: r#"<div data-computed:fullName="$firstName + ' ' + $lastName"></div>"#,
            }][..],
        ),
        (
            "indicator",
            &[Example {
                title: "Loading indicator",
                html: r#"<button data-on:click="@get('/endpoint')" data-indicator:fetching></button>
<div data-show="$fetching">Loading...</div>"#,
            }][..],
        ),
        (
            "on-intersect",
            &[
                Example {
                    title: "Lazy load on visible",
                    html: r#"<div data-intersects="@get('/api/more')">Loading...</div>"#,
                },
                Example {
                    title: "Once intersection",
                    html: r#"<img data-intersects__once="@get('/api/image/$id')" />"#,
                },
            ][..],
        ),
        (
            "scroll-into-view",
            &[
                Example {
                    title: "Smooth scroll",
                    html: r#"<div data-scroll-into-view__smooth>New message</div>"#,
                },
                Example {
                    title: "Instant scroll",
                    html: r#"<div data-scroll-into-view__instant>Jump to here</div>"#,
                },
            ][..],
        ),
        (
            "effect",
            &[Example {
                title: "Side effect on signal change",
                html: r#"<div data-effect="$foo = $bar + $baz"></div>"#,
            }][..],
        ),
        (
            "init",
            &[Example {
                title: "Initialize on load",
                html: r#"<div data-init="$count = 1"></div>"#,
            }][..],
        ),
        (
            "ignore",
            &[Example {
                title: "Ignore element",
                html: r#"<div data-ignore>Datastar skips this element and its children.</div>"#,
            }][..],
        ),
    ]
    .into_iter()
    .collect()
}

/// Get examples for a plugin name. Returns empty slice if none defined.
pub fn for_plugin(name: &str) -> &'static [Example] {
    examples().get(name).copied().unwrap_or(&[])
}

/// Format examples as markdown for hover display.
pub fn format_markdown(name: &str) -> String {
    let exs = for_plugin(name);
    if exs.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\n**Examples:**");
    for (i, ex) in exs.iter().enumerate() {
        out.push_str(&format!(
            "\n{}. {}\n```html\n{}\n```",
            i + 1,
            ex.title,
            ex.html
        ));
    }
    out
}
