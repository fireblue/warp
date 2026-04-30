//! SearchItem implementation for SSH hosts in the command palette.

use fuzzy_match::FuzzyMatchResult;
use ordered_float::OrderedFloat;
use warp_core::ui::color::hex_color::coloru_from_hex_string;
use warp_core::ui::Icon;
use warpui::elements::{
    ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Flex, Highlight, MainAxisSize,
    ParentElement, Radius, Rect, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::{AppContext, Element, SingletonEntity};

use crate::appearance::Appearance;
use crate::search::command_search::searcher::CommandSearchItemAction;
use crate::search::result_renderer::ItemHighlightState;
use crate::search::SearchItem;
use crate::ssh_manager::model::HostAnnotation;

/// One SSH host row in the palette.
#[derive(Debug, Clone)]
pub struct SshSearchItem {
    /// `Host` alias from ssh_config — the value passed to `ssh -t <alias>`.
    pub alias: String,
    /// Resolved `HostName` if any, shown as a secondary subtitle.
    pub hostname: Option<String>,
    /// Optional annotation joined onto this row (color, tags, notes,
    /// last-connected timestamp). `None` if the user never annotated this
    /// alias.
    pub annotation: Option<HostAnnotation>,
    /// Fuzzy match against the alias for the current query.
    pub match_result: FuzzyMatchResult,
}

impl SearchItem for SshSearchItem {
    type Action = CommandSearchItemAction;

    fn render_icon(
        &self,
        highlight_state: ItemHighlightState,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let size = appearance.monospace_font_size();
        let color = highlight_state.icon_fill(appearance);
        Container::new(
            ConstrainedBox::new(Icon::Globe.to_warpui_icon(color).finish())
                .with_width(size)
                .with_height(size)
                .finish(),
        )
        .with_margin_right(12.)
        .finish()
    }

    fn render_item(
        &self,
        highlight_state: ItemHighlightState,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let main_color = highlight_state.main_text_fill(appearance).into_solid();
        let sub_color = highlight_state.sub_text_fill(appearance).into_solid();
        let font_size = appearance.monospace_font_size();

        let alias_text = Text::new_inline(
            self.alias.clone(),
            appearance.monospace_font_family(),
            font_size,
        )
        .with_color(main_color.clone())
        .with_single_highlight(
            Highlight::new()
                .with_properties(Properties::default().weight(Weight::Bold))
                .with_foreground_color(main_color),
            self.match_result.matched_indices.clone(),
        )
        .finish();

        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Max)
            .with_child(alias_text);

        // Color dot — sits immediately after the alias if a color is set.
        if let Some(color) = self
            .annotation
            .as_ref()
            .and_then(|ann| ann.color)
            .and_then(|slug| coloru_from_hex_string(&format!("#{}", slug.hex())).ok())
        {
            let dot_size = font_size * 0.55;
            let dot = ConstrainedBox::new(
                Rect::new()
                    .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                    .with_background_color(color)
                    .finish(),
            )
            .with_width(dot_size)
            .with_height(dot_size)
            .finish();
            let dot_wrapper = Container::new(dot).with_margin_left(8.).finish();
            row.add_child(dot_wrapper);
        }

        if let Some(host) = &self.hostname {
            let host_label = Container::new(
                Text::new_inline(
                    format!("  {host}"),
                    appearance.ui_font_family(),
                    font_size - 1.,
                )
                .with_color(sub_color.clone())
                .finish(),
            )
            .finish();
            row.add_child(host_label);
        }

        // Tag chips. Render at most 3 tags inline to keep rows compact;
        // overflow is left to the annotation editor.
        if let Some(tags) = self.annotation.as_ref().map(|ann| ann.tags.as_slice()) {
            for tag in tags.iter().take(3) {
                let chip = Container::new(
                    Text::new_inline(
                        tag.clone(),
                        appearance.ui_font_family(),
                        font_size - 2.,
                    )
                    .with_color(sub_color.clone())
                    .finish(),
                )
                .with_horizontal_padding(6.)
                .with_vertical_padding(1.)
                .with_background(appearance.theme().surface_2().into_solid())
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
                .with_margin_left(6.)
                .finish();
                row.add_child(chip);
            }
        }

        // Notes preview (truncated, dimmed) trails everything else.
        if let Some(notes) = self
            .annotation
            .as_ref()
            .and_then(|ann| ann.notes.as_deref())
            .filter(|s| !s.is_empty())
        {
            let preview: String = notes.chars().take(40).collect();
            let suffix = if notes.chars().count() > 40 { "…" } else { "" };
            let notes_label = Container::new(
                Text::new_inline(
                    format!("  {preview}{suffix}"),
                    appearance.ui_font_family(),
                    font_size - 2.,
                )
                .with_color(sub_color)
                .finish(),
            )
            .finish();
            row.add_child(notes_label);
        }

        row.finish()
    }

    fn score(&self) -> OrderedFloat<f64> {
        OrderedFloat(self.match_result.score as f64)
    }

    fn accept_result(&self) -> CommandSearchItemAction {
        CommandSearchItemAction::ConnectSshHost(self.alias.clone())
    }

    fn execute_result(&self) -> CommandSearchItemAction {
        // Modified Enter (Cmd+Enter on macOS / Shift+Enter elsewhere) opens
        // the connection in a new window instead of a new tab.
        CommandSearchItemAction::ConnectSshHostInWindow(self.alias.clone())
    }

    fn accessibility_label(&self) -> String {
        match &self.hostname {
            Some(h) => format!("SSH host: {} ({h})", self.alias),
            None => format!("SSH host: {}", self.alias),
        }
    }
}
