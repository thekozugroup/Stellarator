//! The run's tools in its chip row: a chip "5 tools" whose popover lists the servers the run sends
//! tools from (as they were when the run was made) and each tool's name as the model sees it, with
//! the server's own name after a renamed one (`docs__search`, search).

use nsh_base::popover;
use nsh_core::prelude::*;

use crate::api::types::ToolSnapshot;
use crate::format::count;
use crate::screens::tools::logic::transport_label;
use crate::theme::radius;
use crate::theme::text::{label, meta, mono};
use crate::ui::chip::chip;
use crate::ui::material::{material_float, material_thick};

/// How many tools the run sends.
pub fn tool_total(snapshot: &ToolSnapshot) -> usize {
    snapshot.servers.iter().map(|s| s.tools.len()).sum()
}

/// The chip and its popover.
pub fn tools_chip(snapshot: ToolSnapshot) -> impl IntoElement {
    let n = tool_total(&snapshot);
    let trigger = chip(count(n as i64, "tool", "tools")).popover_trigger();
    let servers: Vec<AnyElement> = snapshot
        .servers
        .iter()
        .map(|s| {
            let tools: Vec<AnyElement> = s
                .tools
                .iter()
                .map(|t| {
                    view()
                        .flex_row()
                        .items_baseline()
                        .gap(px(8.0))
                        .min_w(px(0.0))
                        .child(mono(text(t.name.clone())).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
                        .child((t.name != t.server_tool).then(|| meta(text(t.server_tool.clone())).number_of_lines(1)))
                        .into_element()
                })
                .collect();
            view()
                .gap(px(6.0))
                .child(
                    view()
                        .flex_row()
                        .items_baseline()
                        .gap(px(8.0))
                        .child(label(text(s.name.clone())).number_of_lines(1).flex_shrink(1.0))
                        .child(meta(text(format!("{} · {}", transport_label(s.transport), count(s.tools.len() as i64, "tool", "tools"))))),
                )
                .child(view().gap(px(4.0)).children(tools))
                .into_element()
        })
        .collect();
    let panel = material_float(material_thick(view().w(px(320.0)).max_h(px(420.0)).rounded(px(radius::LG)).overflow_hidden()))
        .role(AccessibilityRole::Dialog)
        .label("Tools this run sends")
        .child(scroll().child(view().p(px(14.0)).gap(px(16.0)).children(servers)));
    popover().placement(Placement::Bottom).align(Align::Start).offset(px(6.0)).margin(px(16.0)).trigger(trigger).child(panel)
}
