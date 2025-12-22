use itertools::Itertools;
use re_viewport_blueprint::ViewPropertyQueryError;
use rerun::{
    Component, ComponentType, Text, external::{
        egui::{self, Color32}, re_chunk_store, re_entity_db::InstancePath, re_renderer, re_view::{RangeResultsExt, range_with_blueprint_resolved_data}, re_viewer_context::{
            self, IdentifiedViewSystem, ViewContext, ViewQuery, ViewSystemExecutionError,
            VisualizerQueryInfo, VisualizerSystem, auto_color_for_entity_path,
        }
    }
};

use crate::{
    PlotTextSeries,
    util::{self, get_entity_components, get_label},
};

#[derive(Default)]
pub struct SeriesSpanSystem {
    pub all_series: Vec<PlotTextSeries>,
}

impl IdentifiedViewSystem for SeriesSpanSystem {
    fn identifier() -> re_viewer_context::ViewSystemIdentifier {
        "GenericSeriesSpan".into()
    }
}

impl VisualizerSystem for SeriesSpanSystem {
    fn visualizer_query_info(&self) -> VisualizerQueryInfo {
        VisualizerQueryInfo::empty()
    }

    fn execute(
        &mut self,
        ctx: &ViewContext<'_>,
        query: &ViewQuery<'_>,
        _context: &re_viewer_context::ViewContextCollection,
    ) -> Result<Vec<re_renderer::QueueableDrawData>, ViewSystemExecutionError> {
        self.load_text(ctx, query)?;
        Ok(Vec::new())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SeriesSpanSystem {
    fn load_text(
        &mut self,
        ctx: &ViewContext<'_>,
        query: &ViewQuery<'_>,
    ) -> Result<(), ViewPropertyQueryError> {
        let data_results = query.iter_visible_data_results(Self::identifier());

        let mut series = Default::default();

        for data_result in data_results {
            Self::load_text_series(ctx, query, data_result, &mut series)?;
        }

        self.all_series = series;

        Ok(())
    }

    pub fn component_type() -> ComponentType {
        Text::name()
    }

    fn load_text_series(
        ctx: &ViewContext<'_>,
        view_query: &ViewQuery<'_>,
        data_result: &re_viewer_context::DataResult,
        all_series: &mut Vec<PlotTextSeries>,
    ) -> Result<(), ViewPropertyQueryError> {

        let time_range = util::determine_time_range(ctx, data_result)?;
        let entity_path = &data_result.entity_path;
        let query = re_chunk_store::RangeQuery::new(view_query.timeline, time_range)
            // We must fetch data with extended bounds, otherwise the query clamping would
            // cut-off the data early at the edge of the view.
            .include_extended_bounds(true);

        // Get all components associated with our entity
        let entity_components = get_entity_components(ctx, entity_path, Self::component_type());

        let results = range_with_blueprint_resolved_data(
            ctx,
            None,
            &query,
            data_result,
            entity_components.clone(),
        );
        let timeline = query.timeline;

        let num_series = entity_components
            .iter()
            .filter_map(|component| results.get_required_chunks(component.to_owned()))
            .count();

        for (instance, component) in entity_components.into_iter().enumerate() {
            let Some(_all_text_chunks) = results.get_required_chunks(component) else {
                continue;
            };

            // let times = all_text_chunks.iter()
            //             .flat_map(|chunk| chunk.iter_component_timepoints());

            let strings_chunks = results
                .iter_as(timeline.to_owned(), component);
            
            let strings = strings_chunks.slice::<String>();

            let label = get_label(entity_path, &component);
            // TODO:
            let instance_path = if num_series == 1 {
                InstancePath::entity_all(data_result.entity_path.clone())
            } else {
                InstancePath::instance(data_result.entity_path.clone(), instance as u64)
            };

            let points_all_times = strings
                .map(|((data_time, _row_id), entries)| {
                    (data_time.as_i64(), entries.concat().to_string())
                }).collect_vec();
            
            let points_filtered_inner = points_all_times.iter()
                .zip(points_all_times.iter().skip(1))
                .filter(|((_time_last, label_last), (_time, label))| {
                    label != label_last
                })
                .map(|(_a, b)| b.to_owned());
            
            let points = std::iter::once(points_all_times[0].clone())
                .chain(points_filtered_inner)
                .chain(std::iter::once(points_all_times.last().unwrap().to_owned()))
                .collect_vec();

            let colors: Vec<_> = points.iter().map(|(_time, label)| {
                let mut id = label.to_string();
                id.push_str(component.as_str());
              Color32::from(auto_color_for_entity_path(&id.into())).gamma_multiply(0.15)
            }).collect();

            all_series.push(PlotTextSeries {
                id: egui::Id::new(&instance_path),
                visible: true,
                kind: crate::TextSeriesKind::Spans,
                instance_path,
                label,
                points,
                component_identifier: component,
                colors
            });
        }

        Ok(())
    }
}
