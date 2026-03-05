+++
title = "whereat"
description = "Production error tracing without debuginfo, panic, or overhead"
template = "landing.html"

[extra]
section_order = ["hero", "features", "easy_command", "final_cta"]

[extra.hero]
title = "whereat"
description = "Know where the bug is at() — without panic!, debuginfo, or overhead. Replace ? with .at()? in your call tree for beautiful, async-friendly stacktraces with GitHub links."
badge = "Rust 1.85+ · no_std"
gradient_opacity = 15
cta_buttons = [
    { text = "Get Started", url = "/whereat/getting-started/installation/", style = "primary" },
    { text = "View on GitHub", url = "https://github.com/lilith/whereat", style = "secondary" },
]

[[extra.features]]
title = "Zero Ok-Path Cost"
desc = "No heap allocation, no work, no overhead when your code succeeds. Tracing only happens when errors propagate."
icon = "fa-solid fa-bolt"

[[extra.features]]
title = "Tiny sizeof"
desc = "At<E> is sizeof(E) + 8 bytes. One pointer for a boxed trace. Your error enum stays the same size it was, plus a pointer."
icon = "fa-solid fa-minimize"

[[extra.features]]
title = "100x Faster Than Backtrace"
desc = "18ns per frame vs 2,500ns+ for backtrace/anyhow with RUST_BACKTRACE=1. Captures file:line:col at compile time via #[track_caller]."
icon = "fa-solid fa-gauge-high"

[[extra.features]]
title = "no_std Compatible"
desc = "Works with core + alloc. No std required. Use it in embedded, WASM, or anywhere you need error context without a full standard library."
icon = "fa-solid fa-microchip"

[[extra.features]]
title = "Cross-Crate Tracing"
desc = "at!() and at_crate!() macros capture crate metadata for GitHub, GitLab, Gitea, and Bitbucket links. See exactly which crate each frame came from."
icon = "fa-solid fa-link"

[[extra.features]]
title = "Works With Everything"
desc = "Compatible with plain enums, structs, thiserror, anyhow, or any type with Debug. No changes to your error types required."
icon = "fa-solid fa-puzzle-piece"

[extra.easy_command_section]
title = "Quick Start"
description = "Add whereat to your project and start tracing errors."
tabs = [
    { name = "Cargo.toml", command = "[dependencies]\nwhereat = \"0.1\"" },
    { name = "With Links", command = "[dependencies]\nwhereat = \"0.1\"\n\n# In lib.rs:\n# whereat::define_at_crate_info!();" },
    { name = "docs.rs", link = "https://docs.rs/whereat" },
]

[extra.final_cta_section]
title = "Start Tracing Errors"
description = "whereat is lightweight, production-ready, and works with any error type you already have."
button = { text = "Read the Docs", url = "/whereat/" }
+++
