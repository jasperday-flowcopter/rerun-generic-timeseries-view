//! Column visibility in the generic time series blueprint

use rerun::external::{
    egui::{self, PopupCloseBehavior, containers::menu::{MenuButton, MenuConfig}},
    re_ui::{UiExt, list_item},
    re_viewer_context::{ViewerContext},
};

use crate::view_class::ComponentSettings;

pub fn selection_ui_column_visibility(
    _ctx: &ViewerContext<'_>,
    ui: &'_ mut egui::Ui,
    column_settings: &'_ [ComponentSettings],
) -> Vec<ComponentSettings> {
    let selected_columns = column_settings
        .iter()
        .filter(|c| c.enabled)
        .cloned()
        .collect::<Vec<_>>();
    let visible_count = selected_columns.len();
    let hidden_count = column_settings.len() - visible_count;
    let visible_count_label = format!("{visible_count} visible, {hidden_count} hidden");

    let mut new_settings: Vec<_> = column_settings.iter().cloned().collect();

    let modal_ui = |ui: &mut egui::Ui| {
        //
        // Summary toggle
        //

        let indeterminate = visible_count != 0 && hidden_count != 0;
        let mut all_enabled = hidden_count == 0;

        if ui
            .checkbox_indeterminate(&mut all_enabled, &visible_count_label, indeterminate)
            .changed()
        {
            if all_enabled {
                new_settings.iter_mut().for_each(|c| {
                    c.enabled = true;
                });
            } else {
                new_settings.iter_mut().for_each(|c| {
                    c.enabled = false;
                });
            }
        }

        ui.add_space(12.0);

        //
        // Component columns
        //

        let mut current_entity = None;
        for column in &mut new_settings {
            if Some(&column.entity_path) != current_entity.as_ref() {
                current_entity = Some(column.entity_path.clone());
                ui.add_space(6.0);
                ui.label(column.entity_path.to_string());
            }

            let mut is_visible = column.enabled;

            if ui
                .re_checkbox(&mut is_visible, column.identifier.as_str())
                .changed()
            {
                column.enabled = is_visible;
            }
        }
    };

    ui.list_item_flat_noninteractive(list_item::PropertyContent::new("Enable Series").value_fn(
        |ui, _| {
            MenuButton::new(&visible_count_label)
                .config(
                    MenuConfig::default().close_behavior(PopupCloseBehavior::CloseOnClickOutside),
                )
                .ui(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, modal_ui)
                });
        },
    ));
    new_settings
}