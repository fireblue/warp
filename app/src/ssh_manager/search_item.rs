//! SearchItem implementation for SSH hosts in the command palette.

use fuzzy_match::FuzzyMatchResult;
use ordered_float::OrderedFloat;
use warp_core::ui::Icon;
use warpui::elements::{
    ConstrainedBox, Container, CrossAxisAlignment, Flex, Highlight, MainAxisSize, ParentElement,
    Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::{AppContext, Element, SingletonEntity};

use crate::appearance::Appearance;
use crate::search::command_search::searcher::CommandSearchItemAction;
use crate::search::result_renderer::ItemHighlightState;
use crate::search::SearchItem;

/// One SSH host row in the palette.
#[derive(Debug, Clone)]
pub struct SshSearchItem {
    /// `Host` alias from ssh_config — the value passed to `ssh -t <alias>`.
    pub alias: String,
    /// Resolved `HostName` if any, shown as a secondary subtitle.
    pub hostname: Option<String>,
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

        let alias_text = Text::new_inline(
            self.alias.clone(),
            appearance.monospace_font_family(),
            appearance.monospace_font_size(),
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

        if let Some(host) = &self.hostname {
            let sub_color = highlight_state.sub_text_fill(appearance).into_solid();
            let host_label = Container::new(
                Text::new_inline(
                    format!("  {host}"),
                    appearance.ui_font_family(),
                    appearance.monospace_font_size() - 1.,
                )
                .with_color(sub_color)
                .finish(),
            )
            .finish();
            row.add_child(host_label);
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
        // Same as accept — there is no "accept without execute" semantic for
        // an SSH host (you connect or you don't).
        self.accept_result()
    }

    fn accessibility_label(&self) -> String {
        match &self.hostname {
            Some(h) => format!("SSH host: {} ({h})", self.alias),
            None => format!("SSH host: {}", self.alias),
        }
    }
}
