use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Event type narrowing: maps DOM event names to their interface and properties.
pub struct EventProp {
    pub name: &'static str,
    pub desc: &'static str,
}

pub fn interface_for(event: &str) -> Option<&'static str> {
    INTERFACES.get(event).copied()
}

pub fn properties_for(event: &str) -> Vec<&'static EventProp> {
    let iface = match INTERFACES.get(event) {
        Some(i) => i,
        None => return vec![],
    };

    let mut props: Vec<&'static EventProp> = Vec::new();
    let mut current = *iface;
    loop {
        if let Some(p) = PROPERTIES.get(current) {
            props.extend(p.iter());
        }
        match PARENTS.get(current) {
            Some(parent) => current = parent,
            None => break,
        }
    }
    props
}

// ── Event → Interface mapping ──

static INTERFACES: LazyLock<BTreeMap<&str, &str>> = LazyLock::new(|| {
    [
        ("click", "MouseEvent"),
        ("dblclick", "MouseEvent"),
        ("mousedown", "MouseEvent"),
        ("mouseup", "MouseEvent"),
        ("mousemove", "MouseEvent"),
        ("mouseover", "MouseEvent"),
        ("mouseout", "MouseEvent"),
        ("mouseenter", "MouseEvent"),
        ("mouseleave", "MouseEvent"),
        ("contextmenu", "MouseEvent"),
        ("keydown", "KeyboardEvent"),
        ("keyup", "KeyboardEvent"),
        ("keypress", "KeyboardEvent"),
        ("focus", "FocusEvent"),
        ("blur", "FocusEvent"),
        ("focusin", "FocusEvent"),
        ("focusout", "FocusEvent"),
        ("input", "InputEvent"),
        ("wheel", "WheelEvent"),
        ("touchstart", "TouchEvent"),
        ("touchend", "TouchEvent"),
        ("touchmove", "TouchEvent"),
        ("touchcancel", "TouchEvent"),
        ("drag", "DragEvent"),
        ("dragstart", "DragEvent"),
        ("dragend", "DragEvent"),
        ("dragenter", "DragEvent"),
        ("dragleave", "DragEvent"),
        ("dragover", "DragEvent"),
        ("drop", "DragEvent"),
        ("animationstart", "AnimationEvent"),
        ("animationend", "AnimationEvent"),
        ("animationiteration", "AnimationEvent"),
        ("transitionend", "TransitionEvent"),
        ("submit", "SubmitEvent"),
        ("pointerdown", "PointerEvent"),
        ("pointerup", "PointerEvent"),
        ("pointermove", "PointerEvent"),
        ("pointerover", "PointerEvent"),
        ("pointerout", "PointerEvent"),
        ("pointerenter", "PointerEvent"),
        ("pointerleave", "PointerEvent"),
        ("pointercancel", "PointerEvent"),
        ("copy", "ClipboardEvent"),
        ("cut", "ClipboardEvent"),
        ("paste", "ClipboardEvent"),
        ("hashchange", "HashChangeEvent"),
        ("pagehide", "PageTransitionEvent"),
        ("pageshow", "PageTransitionEvent"),
        ("progress", "ProgressEvent"),
        ("loadstart", "ProgressEvent"),
    ]
    .into_iter()
    .collect()
});

// ── Interface → parent inheritance ──

static PARENTS: LazyLock<BTreeMap<&str, &str>> = LazyLock::new(|| {
    [
        ("MouseEvent", "Event"),
        ("KeyboardEvent", "Event"),
        ("FocusEvent", "Event"),
        ("InputEvent", "Event"),
        ("WheelEvent", "MouseEvent"),
        ("TouchEvent", "Event"),
        ("DragEvent", "MouseEvent"),
        ("AnimationEvent", "Event"),
        ("TransitionEvent", "Event"),
        ("SubmitEvent", "Event"),
        ("PointerEvent", "MouseEvent"),
        ("ClipboardEvent", "Event"),
        ("HashChangeEvent", "Event"),
        ("PageTransitionEvent", "Event"),
        ("ProgressEvent", "Event"),
    ]
    .into_iter()
    .collect()
});

// ── Interface → properties ──

static PROPERTIES: LazyLock<BTreeMap<&str, &[EventProp]>> = LazyLock::new(|| {
    [
        ("Event", EVENT_PROPS as &[_]),
        ("MouseEvent", MOUSE_PROPS as &[_]),
        ("KeyboardEvent", KB_PROPS as &[_]),
        ("FocusEvent", FOCUS_PROPS as &[_]),
        ("InputEvent", INPUT_PROPS as &[_]),
        ("TouchEvent", TOUCH_PROPS as &[_]),
        ("DragEvent", DRAG_PROPS as &[_]),
        ("WheelEvent", WHEEL_PROPS as &[_]),
        ("PointerEvent", POINTER_PROPS as &[_]),
        ("AnimationEvent", ANIM_PROPS as &[_]),
        ("TransitionEvent", TRANSITION_PROPS as &[_]),
        ("SubmitEvent", SUBMIT_PROPS as &[_]),
        ("ClipboardEvent", CLIPBOARD_PROPS as &[_]),
        ("HashChangeEvent", HASH_PROPS as &[_]),
        ("PageTransitionEvent", PAGE_PROPS as &[_]),
        ("ProgressEvent", PROGRESS_PROPS as &[_]),
    ]
    .into_iter()
    .collect()
});

const EVENT_PROPS: &[EventProp] = &[
    EventProp {
        name: "type",
        desc: "string — Name of the event",
    },
    EventProp {
        name: "target",
        desc: "EventTarget — Element that triggered event",
    },
    EventProp {
        name: "currentTarget",
        desc: "EventTarget — Element listener is on",
    },
    EventProp {
        name: "bubbles",
        desc: "boolean — Whether event bubbles",
    },
    EventProp {
        name: "cancelable",
        desc: "boolean — Whether cancellable",
    },
    EventProp {
        name: "defaultPrevented",
        desc: "boolean — Whether preventDefault() called",
    },
    EventProp {
        name: "timeStamp",
        desc: "number — Event creation time (ms)",
    },
    EventProp {
        name: "isTrusted",
        desc: "boolean — Whether browser-initiated",
    },
    EventProp {
        name: "preventDefault",
        desc: "() => void — Cancel event",
    },
    EventProp {
        name: "stopPropagation",
        desc: "() => void — Stop propagation",
    },
    EventProp {
        name: "stopImmediatePropagation",
        desc: "() => void — Stop all listeners",
    },
];

const MOUSE_PROPS: &[EventProp] = &[
    EventProp {
        name: "clientX",
        desc: "number — X relative to viewport",
    },
    EventProp {
        name: "clientY",
        desc: "number — Y relative to viewport",
    },
    EventProp {
        name: "pageX",
        desc: "number — X relative to document",
    },
    EventProp {
        name: "pageY",
        desc: "number — Y relative to document",
    },
    EventProp {
        name: "screenX",
        desc: "number — X relative to screen",
    },
    EventProp {
        name: "screenY",
        desc: "number — Y relative to screen",
    },
    EventProp {
        name: "offsetX",
        desc: "number — X relative to target",
    },
    EventProp {
        name: "offsetY",
        desc: "number — Y relative to target",
    },
    EventProp {
        name: "button",
        desc: "number — Button (0=left,1=middle,2=right)",
    },
    EventProp {
        name: "buttons",
        desc: "number — Bitmask of pressed buttons",
    },
    EventProp {
        name: "altKey",
        desc: "boolean — Alt pressed",
    },
    EventProp {
        name: "ctrlKey",
        desc: "boolean — Ctrl pressed",
    },
    EventProp {
        name: "metaKey",
        desc: "boolean — Meta/Cmd pressed",
    },
    EventProp {
        name: "shiftKey",
        desc: "boolean — Shift pressed",
    },
    EventProp {
        name: "relatedTarget",
        desc: "EventTarget|null — Related target",
    },
];

const KB_PROPS: &[EventProp] = &[
    EventProp {
        name: "key",
        desc: "string — Key value ('Enter', 'a', 'ArrowUp')",
    },
    EventProp {
        name: "code",
        desc: "string — Physical code ('KeyA', 'Enter')",
    },
    EventProp {
        name: "altKey",
        desc: "boolean — Alt pressed",
    },
    EventProp {
        name: "ctrlKey",
        desc: "boolean — Ctrl pressed",
    },
    EventProp {
        name: "metaKey",
        desc: "boolean — Meta/Cmd pressed",
    },
    EventProp {
        name: "shiftKey",
        desc: "boolean — Shift pressed",
    },
    EventProp {
        name: "repeat",
        desc: "boolean — Key held down",
    },
    EventProp {
        name: "location",
        desc: "number — Key location (0=std,1=left,2=right)",
    },
    EventProp {
        name: "isComposing",
        desc: "boolean — IME composition active",
    },
];

const FOCUS_PROPS: &[EventProp] = &[EventProp {
    name: "relatedTarget",
    desc: "EventTarget|null — Losing/gaining focus",
}];

const INPUT_PROPS: &[EventProp] = &[
    EventProp {
        name: "data",
        desc: "string|null — Inserted characters",
    },
    EventProp {
        name: "inputType",
        desc: "string — Type ('insertText', etc.)",
    },
    EventProp {
        name: "isComposing",
        desc: "boolean — IME composition active",
    },
];

const TOUCH_PROPS: &[EventProp] = &[
    EventProp {
        name: "touches",
        desc: "TouchList — All touches",
    },
    EventProp {
        name: "targetTouches",
        desc: "TouchList — Touches on target",
    },
    EventProp {
        name: "changedTouches",
        desc: "TouchList — Changed touches",
    },
    EventProp {
        name: "altKey",
        desc: "boolean — Alt pressed",
    },
    EventProp {
        name: "ctrlKey",
        desc: "boolean — Ctrl pressed",
    },
    EventProp {
        name: "metaKey",
        desc: "boolean — Meta/Cmd pressed",
    },
    EventProp {
        name: "shiftKey",
        desc: "boolean — Shift pressed",
    },
];

const DRAG_PROPS: &[EventProp] = &[EventProp {
    name: "dataTransfer",
    desc: "DataTransfer — Drag data",
}];

const WHEEL_PROPS: &[EventProp] = &[
    EventProp {
        name: "deltaX",
        desc: "number — Horizontal scroll",
    },
    EventProp {
        name: "deltaY",
        desc: "number — Vertical scroll",
    },
    EventProp {
        name: "deltaZ",
        desc: "number — Z-axis scroll",
    },
    EventProp {
        name: "deltaMode",
        desc: "number — Unit (0=pixel,1=line,2=page)",
    },
];

const POINTER_PROPS: &[EventProp] = &[
    EventProp {
        name: "pointerId",
        desc: "number — Unique pointer ID",
    },
    EventProp {
        name: "width",
        desc: "number — Contact width",
    },
    EventProp {
        name: "height",
        desc: "number — Contact height",
    },
    EventProp {
        name: "pressure",
        desc: "number — Pressure (0-1)",
    },
    EventProp {
        name: "pointerType",
        desc: "string — 'mouse','pen','touch'",
    },
    EventProp {
        name: "isPrimary",
        desc: "boolean — Primary pointer",
    },
];

const ANIM_PROPS: &[EventProp] = &[
    EventProp {
        name: "animationName",
        desc: "string — CSS animation name",
    },
    EventProp {
        name: "elapsedTime",
        desc: "number — Time elapsed (s)",
    },
    EventProp {
        name: "pseudoElement",
        desc: "string — Target pseudo-element",
    },
];

const TRANSITION_PROPS: &[EventProp] = &[
    EventProp {
        name: "propertyName",
        desc: "string — CSS property name",
    },
    EventProp {
        name: "elapsedTime",
        desc: "number — Time elapsed (s)",
    },
    EventProp {
        name: "pseudoElement",
        desc: "string — Target pseudo-element",
    },
];

const SUBMIT_PROPS: &[EventProp] = &[EventProp {
    name: "submitter",
    desc: "HTMLElement|null — Submit trigger",
}];

const CLIPBOARD_PROPS: &[EventProp] = &[EventProp {
    name: "clipboardData",
    desc: "DataTransfer — Clipboard data",
}];

const HASH_PROPS: &[EventProp] = &[
    EventProp {
        name: "oldURL",
        desc: "string — Previous URL",
    },
    EventProp {
        name: "newURL",
        desc: "string — New URL",
    },
];

const PAGE_PROPS: &[EventProp] = &[EventProp {
    name: "persisted",
    desc: "boolean — Loaded from cache",
}];

const PROGRESS_PROPS: &[EventProp] = &[
    EventProp {
        name: "lengthComputable",
        desc: "boolean — Total known",
    },
    EventProp {
        name: "loaded",
        desc: "number — Amount loaded",
    },
    EventProp {
        name: "total",
        desc: "number — Total amount",
    },
];
