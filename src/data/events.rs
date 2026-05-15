/// Common HTML DOM events used with `data-on:`.
pub const KNOWN_DOM_EVENTS: &[&str] = &[
    // Mouse events
    "click",
    "dblclick",
    "contextmenu",
    "mousedown",
    "mouseup",
    "mousemove",
    "mouseenter",
    "mouseleave",
    "mouseover",
    "mouseout",
    "wheel",
    // Keyboard events
    "keydown",
    "keyup",
    "keypress",
    // Form events
    "focus",
    "blur",
    "change",
    "input",
    "submit",
    "reset",
    "select",
    // Window/document events
    "load",
    "unload",
    "beforeunload",
    "scroll",
    "resize",
    "error",
    // Touch events
    "touchstart",
    "touchend",
    "touchmove",
    "touchcancel",
    // Pointer events
    "pointerdown",
    "pointerup",
    "pointermove",
    "pointerenter",
    "pointerleave",
    "pointercancel",
    // Drag events
    "drag",
    "dragstart",
    "dragend",
    "dragenter",
    "dragleave",
    "dragover",
    "drop",
    // Clipboard events
    "copy",
    "cut",
    "paste",
    // Media events
    "play",
    "pause",
    "ended",
    "volumechange",
    "timeupdate",
    // Animation/transition
    "animationend",
    "animationstart",
    "transitionend",
    // Datastar custom events
    "datastar-fetch",
    "rocket-launched",
];
