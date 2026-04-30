//! Settings → SSH profiles page.
//!
//! Lists every host parsed out of `~/.warp/ssh_config` (which `Include`s the
//! user's `~/.ssh/config`), labelled by source. For each row the user can:
//! - **Annotate**: edit color/tags/notes (writes into the Warp DB).
//! - **Delete**: remove a Warp-managed host from `~/.warp/ssh_config`. User-
//!   defined hosts are read-only here — the user edits their own config file
//!   to change them.
//!
//! Reads are synchronous off `~/.warp/ssh_config` and the read-only sqlite
//! connection. Writes go through [`crate::persistence::PersistenceWriter`]
//! via [`ModelEvent`] to keep a single writer of the database.

use std::collections::HashMap;

use warp_core::ui::color::hex_color::coloru_from_hex_string;
use warpui::elements::{
    ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Element, Flex, MainAxisSize,
    MouseStateHandle, ParentElement, Radius, Rect, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::ui_components::button::ButtonVariant;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::{AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle};

use super::settings_page::{
    MatchData, PageType, SettingsPageEvent, SettingsPageMeta, SettingsPageViewHandle,
    SettingsWidget,
};
use super::SettingsSection;
use crate::appearance::Appearance;
use crate::persistence::{ModelEvent, PersistenceWriter};
use crate::ssh_manager::model::{ColorSlug, HostAnnotation, HostSource, SshHost};
use crate::ssh_manager::{annotation_repo, parser, paths};

/// Actions emitted by clicks within the SSH profiles page.
#[derive(Debug, Clone)]
pub enum SshProfilesAction {
    /// Toggle the inline annotation editor for a host alias.
    ToggleEditAnnotation(String),
    /// Cycle the editor's selected color forward (or to clear).
    SetEditingColor(Option<ColorSlug>),
    /// Save the in-progress annotation back to the database.
    SaveAnnotation,
    /// Discard editor changes and collapse the row.
    CancelEdit,
}

pub struct SshProfilesPageView {
    page: PageType<Self>,
    /// Alias whose annotation is currently being edited inline. `None` when
    /// the page is in its read-only listing mode.
    editing_alias: Option<String>,
    /// Working annotation buffer for the row currently being edited. Kept in
    /// view state so click handlers can read/mutate it without going to the
    /// DB on every keystroke.
    editing_annotation: Option<HostAnnotation>,
}

impl SshProfilesPageView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self {
            page: PageType::new_uncategorized(vec![Box::new(SshProfilesWidget::default())], None),
            editing_alias: None,
            editing_annotation: None,
        }
    }

    fn begin_edit(&mut self, alias: &str, ctx: &mut ViewContext<Self>) {
        let annotation = load_annotations()
            .remove(alias)
            .unwrap_or_default();
        self.editing_alias = Some(alias.to_owned());
        self.editing_annotation = Some(annotation);
        ctx.notify();
    }

    fn cancel_edit(&mut self, ctx: &mut ViewContext<Self>) {
        self.editing_alias = None;
        self.editing_annotation = None;
        ctx.notify();
    }

    fn save_edit(&mut self, ctx: &mut ViewContext<Self>) {
        let (Some(alias), Some(annotation)) =
            (self.editing_alias.take(), self.editing_annotation.take())
        else {
            return;
        };

        let sender = PersistenceWriter::handle(ctx).as_ref(ctx).sender();
        if let Some(sender) = sender {
            let _ = sender.send(ModelEvent::SshAnnotationUpsert { alias, annotation });
        } else {
            log::warn!("ssh_profiles: no persistence sender available; annotation not saved");
        }
        ctx.notify();
    }
}

impl Entity for SshProfilesPageView {
    type Event = SettingsPageEvent;
}

impl TypedActionView for SshProfilesPageView {
    type Action = SshProfilesAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SshProfilesAction::ToggleEditAnnotation(alias) => {
                if self.editing_alias.as_deref() == Some(alias.as_str()) {
                    self.cancel_edit(ctx);
                } else {
                    self.begin_edit(alias, ctx);
                }
            }
            SshProfilesAction::SetEditingColor(color) => {
                if let Some(annotation) = self.editing_annotation.as_mut() {
                    annotation.color = *color;
                    ctx.notify();
                }
            }
            SshProfilesAction::SaveAnnotation => self.save_edit(ctx),
            SshProfilesAction::CancelEdit => self.cancel_edit(ctx),
        }
    }
}

impl View for SshProfilesPageView {
    fn ui_name() -> &'static str {
        "SshProfilesPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for SshProfilesPageView {
    fn section() -> SettingsSection {
        SettingsSection::SshProfiles
    }

    fn should_render(&self, _ctx: &AppContext) -> bool {
        true
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id)
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

impl From<ViewHandle<SshProfilesPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<SshProfilesPageView>) -> Self {
        SettingsPageViewHandle::SshProfiles(view_handle)
    }
}

#[derive(Default)]
struct SshProfilesWidget {
    /// One mouse state per visible row's "Edit" button. Indexed by host
    /// alias rather than by index because hosts can come and go between
    /// renders if the user edits the underlying ssh_config.
    edit_button_states: std::cell::RefCell<HashMap<String, MouseStateHandle>>,
    save_button_state: MouseStateHandle,
    cancel_button_state: MouseStateHandle,
    color_button_states: std::cell::RefCell<HashMap<ColorSlug, MouseStateHandle>>,
    clear_color_state: MouseStateHandle,
}

impl SshProfilesWidget {
    fn edit_button(&self, alias: &str) -> MouseStateHandle {
        self.edit_button_states
            .borrow_mut()
            .entry(alias.to_owned())
            .or_default()
            .clone()
    }

    fn color_button(&self, slug: ColorSlug) -> MouseStateHandle {
        self.color_button_states
            .borrow_mut()
            .entry(slug)
            .or_default()
            .clone()
    }
}

impl SettingsWidget for SshProfilesWidget {
    type View = SshProfilesPageView;

    fn search_terms(&self) -> &str {
        "ssh profiles hosts connect alias annotation tags color notes"
    }

    fn render(
        &self,
        view: &Self::View,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let main_color = theme
            .main_text_color(theme.surface_2())
            .into_solid();
        let sub_color = theme
            .sub_text_color(theme.surface_2())
            .into_solid();
        let font_size = appearance.monospace_font_size();

        let hosts = match load_hosts() {
            Ok(hosts) => hosts,
            Err(err) => {
                return Container::new(
                    Text::new_inline(
                        format!("Failed to load SSH hosts: {err:#}"),
                        appearance.ui_font_family(),
                        font_size,
                    )
                    .with_color(sub_color)
                    .finish(),
                )
                .with_padding_bottom(8.)
                .finish();
            }
        };
        let annotations = load_annotations();

        let mut column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_main_axis_size(MainAxisSize::Min)
            .with_child(
                Text::new_inline(
                    format!("{} host(s) discovered.", hosts.len()),
                    appearance.ui_font_family(),
                    font_size - 1.,
                )
                .with_color(sub_color.clone())
                .finish(),
            );

        if hosts.is_empty() {
            column = column.with_child(
                Container::new(
                    Text::new_inline(
                        "No hosts found in ~/.ssh/config or ~/.warp/ssh_config."
                            .to_owned(),
                        appearance.ui_font_family(),
                        font_size,
                    )
                    .with_color(main_color)
                    .finish(),
                )
                .with_padding_top(12.)
                .finish(),
            );
            return column.finish();
        }

        for host in hosts {
            let row = self.render_row(view, &host, &annotations, appearance);
            column = column.with_child(row);
        }

        Container::new(column.finish())
            .with_padding_top(12.)
            .with_padding_bottom(12.)
            .finish()
    }
}

impl SshProfilesWidget {
    fn render_row(
        &self,
        view: &SshProfilesPageView,
        host: &SshHost,
        annotations: &HashMap<String, HostAnnotation>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let main_color = theme.main_text_color(theme.surface_2()).into_solid();
        let sub_color = theme.sub_text_color(theme.surface_2()).into_solid();
        let font_size = appearance.monospace_font_size();
        let is_editing = view.editing_alias.as_deref() == Some(host.alias.as_str());
        let display_annotation = if is_editing {
            view.editing_annotation.as_ref()
        } else {
            annotations.get(&host.alias)
        };

        // Header row: alias | source | hostname | edit button.
        let mut header = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Max);

        // Color dot.
        if let Some(color_u) = display_annotation
            .and_then(|ann| ann.color)
            .and_then(|slug| coloru_from_hex_string(&format!("#{}", slug.hex())).ok())
        {
            let dot_size = font_size * 0.55;
            let dot = ConstrainedBox::new(
                Rect::new()
                    .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                    .with_background_color(color_u)
                    .finish(),
            )
            .with_width(dot_size)
            .with_height(dot_size)
            .finish();
            header.add_child(
                Container::new(dot)
                    .with_margin_right(8.)
                    .finish(),
            );
        }

        header.add_child(
            Text::new_inline(
                host.alias.clone(),
                appearance.monospace_font_family(),
                font_size,
            )
            .with_color(main_color.clone())
            .with_style(Properties::default().weight(Weight::Bold))
            .finish(),
        );

        let source_label = match host.source {
            HostSource::User => "user",
            HostSource::Warp => "warp",
        };
        header.add_child(
            Container::new(
                Text::new_inline(
                    source_label.to_owned(),
                    appearance.ui_font_family(),
                    font_size - 2.,
                )
                .with_color(sub_color.clone())
                .finish(),
            )
            .with_horizontal_padding(6.)
            .with_vertical_padding(1.)
            .with_background(theme.surface_3().into_solid())
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
            .with_margin_left(8.)
            .finish(),
        );

        if let Some(host_name) = &host.hostname {
            header.add_child(
                Container::new(
                    Text::new_inline(
                        host_name.clone(),
                        appearance.ui_font_family(),
                        font_size - 1.,
                    )
                    .with_color(sub_color.clone())
                    .finish(),
                )
                .with_margin_left(12.)
                .finish(),
            );
        }

        // Edit/Cancel button on the right.
        let alias_for_action = host.alias.clone();
        let button_label = if is_editing { "Cancel" } else { "Edit" };
        let edit_button = appearance
            .ui_builder()
            .button(ButtonVariant::Text, self.edit_button(&host.alias))
            .with_text_label(button_label.to_owned())
            .with_style(
                UiComponentStyles::default()
                    .set_padding(Coords::default().left(8.).right(8.).top(4.).bottom(4.))
                    .set_margin(Coords::default().left(8.)),
            )
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(SshProfilesAction::ToggleEditAnnotation(
                    alias_for_action.clone(),
                ));
            })
            .finish();
        header.add_child(edit_button);

        let mut row_column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_main_axis_size(MainAxisSize::Min)
            .with_child(header.finish());

        // Annotation summary line: tags + notes preview, only when not in edit mode.
        if !is_editing {
            if let Some(annotation) = display_annotation {
                if !annotation.tags.is_empty() || annotation.notes.is_some() {
                    row_column = row_column
                        .with_child(self.render_annotation_summary(annotation, appearance));
                }
            }
        } else {
            row_column = row_column.with_child(self.render_inline_editor(appearance));
        }

        Container::new(row_column.finish())
            .with_padding_top(8.)
            .with_padding_bottom(8.)
            .finish()
    }

    fn render_annotation_summary(
        &self,
        annotation: &HostAnnotation,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let sub_color = theme.sub_text_color(theme.surface_2()).into_solid();
        let font_size = appearance.monospace_font_size();

        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Min);

        for tag in annotation.tags.iter().take(5) {
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
            .with_background(theme.surface_2().into_solid())
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
            .with_margin_right(6.)
            .finish();
            row.add_child(chip);
        }

        if let Some(notes) = annotation.notes.as_deref().filter(|s| !s.is_empty()) {
            let preview: String = notes.chars().take(80).collect();
            let suffix = if notes.chars().count() > 80 { "…" } else { "" };
            row.add_child(
                Text::new_inline(
                    format!("{preview}{suffix}"),
                    appearance.ui_font_family(),
                    font_size - 2.,
                )
                .with_color(sub_color)
                .finish(),
            );
        }

        Container::new(row.finish())
            .with_padding_left(20.)
            .with_padding_top(4.)
            .finish()
    }

    fn render_inline_editor(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let main_color = theme.main_text_color(theme.surface_2()).into_solid();
        let font_size = appearance.monospace_font_size();

        // Color picker — circles for each preset, plus a "clear" pill.
        let mut color_row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Min)
            .with_child(
                Container::new(
                    Text::new_inline(
                        "Color".to_owned(),
                        appearance.ui_font_family(),
                        font_size - 1.,
                    )
                    .with_color(main_color.clone())
                    .finish(),
                )
                .with_margin_right(8.)
                .finish(),
            );

        for slug in ColorSlug::all() {
            let slug = *slug;
            let color_u = match coloru_from_hex_string(&format!("#{}", slug.hex())) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let dot_size = font_size * 0.85;
            let dot = ConstrainedBox::new(
                Rect::new()
                    .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                    .with_background_color(color_u)
                    .finish(),
            )
            .with_width(dot_size)
            .with_height(dot_size)
            .finish();
            let clickable = Container::new(dot)
                .with_horizontal_padding(4.)
                .with_vertical_padding(4.)
                .finish();
            // We rely on the surrounding hoverable button infra to make the
            // dot clickable. For now use a plain button label so each slug
            // is distinguishable; real hover affordance is a follow-up.
            let button = appearance
                .ui_builder()
                .button(ButtonVariant::Text, self.color_button(slug))
                .with_style(
                    UiComponentStyles::default()
                        .set_padding(Coords::uniform(2.))
                        .set_margin(Coords::default().right(4.)),
                )
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(SshProfilesAction::SetEditingColor(Some(slug)));
                })
                .finish();
            color_row = color_row.with_child(clickable).with_child(button);
        }

        let clear_button = appearance
            .ui_builder()
            .button(ButtonVariant::Text, self.clear_color_state.clone())
            .with_text_label("Clear".to_owned())
            .with_style(
                UiComponentStyles::default()
                    .set_padding(Coords::default().left(8.).right(8.).top(2.).bottom(2.))
                    .set_margin(Coords::default().left(8.)),
            )
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(SshProfilesAction::SetEditingColor(None));
            })
            .finish();
        color_row = color_row.with_child(clear_button);

        let mut button_row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Min);

        button_row.add_child(
            appearance
                .ui_builder()
                .button(ButtonVariant::Accent, self.save_button_state.clone())
                .with_text_label("Save".to_owned())
                .with_style(
                    UiComponentStyles::default()
                        .set_padding(Coords::default().left(12.).right(12.).top(4.).bottom(4.))
                        .set_margin(Coords::default().right(8.)),
                )
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(SshProfilesAction::SaveAnnotation);
                })
                .finish(),
        );
        button_row.add_child(
            appearance
                .ui_builder()
                .button(ButtonVariant::Text, self.cancel_button_state.clone())
                .with_text_label("Cancel".to_owned())
                .with_style(
                    UiComponentStyles::default()
                        .set_padding(Coords::default().left(12.).right(12.).top(4.).bottom(4.)),
                )
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(SshProfilesAction::CancelEdit);
                })
                .finish(),
        );

        Container::new(
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .with_main_axis_size(MainAxisSize::Min)
                .with_child(color_row.finish())
                .with_child(
                    Container::new(
                        Text::new_inline(
                            "Tags and notes editing coming soon — for now the color picker can be saved.".to_owned(),
                            appearance.ui_font_family(),
                            font_size - 2.,
                        )
                        .with_color(theme.sub_text_color(theme.surface_2()).into_solid())
                        .finish(),
                    )
                    .with_padding_top(6.)
                    .with_padding_bottom(6.)
                    .finish(),
                )
                .with_child(button_row.finish())
                .finish(),
        )
        .with_padding_left(20.)
        .with_padding_top(8.)
        .with_padding_bottom(8.)
        .finish()
    }
}

fn load_hosts() -> anyhow::Result<Vec<SshHost>> {
    let path = paths::ensure_warp_config()?;
    let mut hosts = parser::parse_all(&path)?;
    hosts.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(hosts)
}

fn load_annotations() -> HashMap<String, HostAnnotation> {
    #[cfg(feature = "local_fs")]
    {
        let Some(db_url) = crate::persistence::database_file_path()
            .to_str()
            .map(str::to_owned)
        else {
            return HashMap::new();
        };
        match crate::persistence::establish_ro_connection(&db_url) {
            Ok(mut conn) => match annotation_repo::list_all(&mut conn) {
                Ok(entries) => entries.into_iter().collect(),
                Err(err) => {
                    log::warn!("ssh_profiles: annotation list_all failed: {err:#}");
                    HashMap::new()
                }
            },
            Err(err) => {
                log::warn!("ssh_profiles: RO connection failed: {err:#}");
                HashMap::new()
            }
        }
    }
    #[cfg(not(feature = "local_fs"))]
    {
        HashMap::new()
    }
}
