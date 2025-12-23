use itertools::{Either, Itertools as _};

use rerun::external::re_chunk_store::{self, LatestAtQuery};
use rerun::external::re_sdk_types::{
    Archetype as _, archetypes,
    components::{Color, MarkerShape, MarkerSize},
};
use rerun::external::re_view::{
    self, clamped_or_nothing, latest_at_with_blueprint_resolved_data,
    range_with_blueprint_resolved_data,
};
use rerun::external::re_viewer_context::{
    IdentifiedViewSystem, ViewContext, ViewQuery, ViewSystemExecutionError,
    VisualizerExecutionOutput, VisualizerQueryInfo, VisualizerSystem,
    external::re_entity_db::InstancePath, typed_fallback_for,
};
use rerun::external::{re_query, re_sdk_types, re_viewer_context};
use rerun::{ComponentType, Scalars};

use crate::LoadSeriesError;
use crate::util::{get_entity_components, get_label};
use crate::{
    PlotPoint, PlotPointAttrs, PlotSeries, PlotSeriesKind, ScatterAttrs,
    series_query::{
        all_scalars_indices, allocate_plot_points, collect_colors, collect_radius_ui,
        collect_scalars, collect_series_visibility, determine_num_series,
    },
    util,
};

/// The system for rendering [`archetypes::SeriesPoints`] archetypes.
#[derive(Default, Debug)]
pub struct SeriesPointsSystem {
    pub all_series: Vec<PlotSeries>,
}

impl IdentifiedViewSystem for SeriesPointsSystem {
    fn identifier() -> re_viewer_context::ViewSystemIdentifier {
        "GenericSeriesPoints".into()
    }
}

impl VisualizerSystem for SeriesPointsSystem {
    fn visualizer_query_info(&self) -> VisualizerQueryInfo {
        VisualizerQueryInfo::empty()
    }

    fn execute(
        &mut self,
        ctx: &ViewContext<'_>,
        query: &ViewQuery<'_>,
        _context: &re_viewer_context::ViewContextCollection,
    ) -> Result<VisualizerExecutionOutput, ViewSystemExecutionError> {
        // re_tracing::profile_function!();

        self.load_scalars(ctx, query)?;
        Ok(VisualizerExecutionOutput::default())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SeriesPointsSystem {
    fn load_scalars(
        &mut self,
        ctx: &ViewContext<'_>,
        query: &ViewQuery<'_>,
    ) -> Result<VisualizerExecutionOutput, ViewSystemExecutionError> {
        // re_tracing::profile_function!();

        let plot_mem =
            egui_plot::PlotMemory::load(ctx.viewer_ctx.egui_ctx(), crate::plot_id(query.view_id));
        let time_per_pixel = util::determine_time_per_pixel(ctx.viewer_ctx, plot_mem.as_ref());

        let data_results = query.iter_visible_data_results(Self::identifier());

        use rayon::prelude::*;

        let mut output = VisualizerExecutionOutput::default();

        // re_tracing::profile_wait!("load_series");
        for result in data_results
            .collect_vec()
            .par_iter()
            .map(|data_result| Self::load_series(ctx, query, time_per_pixel, data_result))
            .collect::<Vec<_>>()
        {
            match result {
                Err(LoadSeriesError::ViewPropertyQuery(err)) => return Err(err.into()),
                Err(LoadSeriesError::EntitySpecificVisualizerError { entity_path, error }) => {
                    output.report_error_for(entity_path, error);
                }
                Ok(one_series) => {
                    self.all_series.extend(one_series);
                }
            }
        }

        Ok(output)
    }

    pub fn component_type() -> ComponentType {
        Scalars::descriptor_scalars().component_type.unwrap()
    }

    fn load_series(
        ctx: &ViewContext<'_>,
        view_query: &ViewQuery<'_>,
        time_per_pixel: f64,
        data_result: &re_viewer_context::DataResult,
    ) -> Result<Vec<PlotSeries>, LoadSeriesError> {
        // re_tracing::profile_function!();

        let current_query = ctx.current_query();
        let query_ctx = ctx.query_context(data_result, &current_query);

        let fallback_shape = MarkerShape::default();

        let time_range = util::determine_time_range(ctx, data_result)?;

        {
            use re_view::RangeResultsExt as _;

            // re_tracing::profile_scope!("primary", &data_result.entity_path.to_string());

            let entity_path = &data_result.entity_path;
            let query = re_chunk_store::RangeQuery::new(view_query.timeline, time_range);

            let entity_components = get_entity_components(ctx, entity_path, Self::component_type());

            let results = range_with_blueprint_resolved_data(
                ctx,
                None,
                &query,
                data_result,
                archetypes::Scalars::all_component_identifiers()
                    .chain(archetypes::SeriesPoints::all_component_identifiers())
                    .chain(entity_components.clone()),
            );

            // If we have no scalars, we can't do anything.
            let all_scalar_chunks_vec = entity_components
                .iter()
                .chain(std::iter::once(
                    &archetypes::Scalars::descriptor_scalars().component,
                ))
                .unique()
                .filter_map(|c_id| results.get_required_chunks(c_id.to_owned()))
                .collect_vec();

            if all_scalar_chunks_vec.is_empty() {
                return Err(LoadSeriesError::EntitySpecificVisualizerError {
                    entity_path: data_result.entity_path.clone(),
                    error: "No valid scalar data found".to_owned(),
                });
            }

            // All the default values for a `PlotPoint`, accounting for both overrides and default values.
            let fallback_color: Color = typed_fallback_for(
                &query_ctx,
                archetypes::SeriesPoints::descriptor_colors().component,
            );
            let fallback_size: MarkerSize = typed_fallback_for(
                &query_ctx,
                archetypes::SeriesPoints::descriptor_marker_sizes().component,
            );
            let default_point = PlotPoint {
                time: 0,
                value: 0.0,
                attrs: PlotPointAttrs {
                    color: fallback_color.into(),
                    // NOTE: arguably, the `MarkerSize` value should be twice the `radius_ui`. We do
                    // stick to the semantics of `MarkerSize` == radius for backward compatibility and
                    // because markers need a decent radius value to be at all legible.
                    radius_ui: **fallback_size,
                    kind: PlotSeriesKind::Scatter(ScatterAttrs {
                        marker: fallback_shape,
                    }),
                },
            };

            let num_series_vec = all_scalar_chunks_vec
                .iter()
                .map(|all_scalar_chunks| determine_num_series(all_scalar_chunks))
                .collect_vec();

            let total_num_series = num_series_vec.iter().sum();

            let mut points_per_series = match all_scalar_chunks_vec.first() {
                Some(all_scalar_chunks) => allocate_plot_points(
                    &query,
                    &default_point,
                    all_scalar_chunks,
                    total_num_series,
                ),
                None => {
                    return Err(LoadSeriesError::EntitySpecificVisualizerError {
                        entity_path: data_result.entity_path.clone(),
                        error: "No points in first series".to_owned(),
                    });
                }
            };

            let mut start_idx = 0;
            for (all_scalar_chunks, n_series) in
                all_scalar_chunks_vec.iter().zip(num_series_vec.iter())
            {
                let end_idx = start_idx + n_series;
                collect_scalars(
                    all_scalar_chunks,
                    &mut points_per_series[start_idx..end_idx],
                );
                start_idx = end_idx;
            }

            // The plot view visualizes scalar data within a specific time range, without any kind
            // of time-alignment / bootstrapping behavior:
            // * For the scalar themselves, this is what you want: if you're trying to plot some
            //   data between t=100 and t=200, you don't want to display a point from t=20 (and
            //   _extended bounds_ will take care of lines crossing the limit).
            // * For the secondary components (colors, radii, names, etc), this is a problem
            //   though: you don't want your plot to change color depending on what the currently
            //   visible time range is! Secondary components have to be bootstrapped.
            let query_shadowed_components = false;
            let bootstrapped_results = latest_at_with_blueprint_resolved_data(
                ctx,
                None,
                &LatestAtQuery::new(query.timeline, query.range.min()),
                data_result,
                archetypes::SeriesPoints::all_component_identifiers(),
                query_shadowed_components,
            );

            collect_colors(
                entity_path,
                &query,
                &bootstrapped_results,
                &results,
                all_scalar_chunks_vec.first().unwrap(),
                &mut points_per_series,
                &archetypes::SeriesPoints::descriptor_colors(),
            );
            collect_radius_ui(
                &query,
                &bootstrapped_results,
                &results,
                all_scalar_chunks_vec.first().unwrap(),
                &mut points_per_series,
                &archetypes::SeriesPoints::descriptor_marker_sizes(),
                // `marker_size` is a radius, see NOTE above
                1.0,
            );

            // Fill in marker shapes
            {
                // re_tracing::profile_scope!("fill marker shapes");

                {
                    let all_marker_shapes_chunks = bootstrapped_results
                        .get_optional_chunks(
                            archetypes::SeriesPoints::descriptor_markers().component,
                        )
                        .iter()
                        .cloned()
                        .chain(
                            results
                                .get_optional_chunks(
                                    archetypes::SeriesPoints::descriptor_markers().component,
                                )
                                .iter()
                                .cloned(),
                        )
                        .collect_vec();

                    if all_marker_shapes_chunks.len() == 1
                        && all_marker_shapes_chunks[0].is_static()
                    {
                        // re_tracing::profile_scope!("override/default fast path");

                        if let Some(marker_shapes) = all_marker_shapes_chunks[0]
                            .iter_component::<MarkerShape>(
                                archetypes::SeriesPoints::descriptor_markers().component,
                            )
                            .next()
                        {
                            for (points, marker_shape) in points_per_series.iter_mut().zip(
                                clamped_or_nothing(marker_shapes.as_slice(), total_num_series),
                            ) {
                                for point in points {
                                    point.attrs.kind = PlotSeriesKind::Scatter(ScatterAttrs {
                                        marker: *marker_shape,
                                    });
                                }
                            }
                        }
                    } else {
                        // re_tracing::profile_scope!("standard path");

                        let mut all_marker_shapes_iters = all_marker_shapes_chunks
                            .iter()
                            .map(|chunk| {
                                chunk.iter_component::<MarkerShape>(
                                    archetypes::SeriesPoints::descriptor_markers().component,
                                )
                            })
                            .collect_vec();
                        let all_marker_shapes_indexed = {
                            let all_marker_shapes = all_marker_shapes_iters
                                .iter_mut()
                                .flat_map(|it| it.into_iter());
                            let all_marker_shapes_indices =
                                all_marker_shapes_chunks.iter().flat_map(|chunk| {
                                    chunk.iter_component_indices(
                                        *query.timeline(),
                                        archetypes::SeriesPoints::descriptor_markers().component,
                                    )
                                });
                            itertools::izip!(all_marker_shapes_indices, all_marker_shapes)
                        };

                        let all_frames = re_query::range_zip_1x1(
                            all_scalars_indices(&query, all_scalar_chunks_vec.first().unwrap()),
                            all_marker_shapes_indexed,
                        )
                        .enumerate();

                        // Simplified path for single series.
                        if total_num_series == 1 {
                            let points = &mut *points_per_series[0];
                            all_frames.for_each(|(i, (_index, _scalars, marker_shapes))| {
                                if let Some(marker) = marker_shapes
                                    .and_then(|marker_shapes| marker_shapes.first().copied())
                                {
                                    points[i].attrs.kind =
                                        PlotSeriesKind::Scatter(ScatterAttrs { marker });
                                }
                            });
                        } else {
                            all_frames.for_each(|(i, (_index, _scalars, marker_shapes))| {
                                if let Some(marker_shapes) = marker_shapes {
                                    for (points, marker) in points_per_series
                                        .iter_mut()
                                        .zip(clamped_or_nothing(&marker_shapes, total_num_series))
                                    {
                                        points[i].attrs.kind =
                                            PlotSeriesKind::Scatter(ScatterAttrs {
                                                marker: *marker,
                                            });
                                    }
                                }
                            });
                        }
                    }
                }
            }

            let series_visibility = collect_series_visibility(
                &query,
                &bootstrapped_results,
                &results,
                total_num_series,
                archetypes::SeriesPoints::descriptor_visible_series().component,
            );
            let series_names = entity_components
                .iter()
                .zip(num_series_vec.iter())
                .flat_map(|(c_id, n_series)| {
                    if *n_series == 1usize {
                        Either::Left(std::iter::once(get_label(entity_path, c_id)))
                    } else {
                        Either::Right(
                            (1..=*n_series)
                                .map(|n| format!("{}.{}", get_label(entity_path, c_id), n)),
                        )
                    }
                })
                .collect_vec();

            let mut series = Vec::with_capacity(total_num_series);

            debug_assert_eq!(points_per_series.len(), series_names.len());
            for (instance, (points, label, visible, component_identifier)) in itertools::izip!(
                points_per_series.into_iter(),
                series_names.into_iter(),
                series_visibility.into_iter(),
                entity_components.into_iter(),
            )
            .enumerate()
            {
                let instance_path = if total_num_series == 1 {
                    InstancePath::entity_all(data_result.entity_path.clone())
                } else {
                    InstancePath::instance(data_result.entity_path.clone(), instance as u64)
                };

                util::points_to_series(
                    instance_path,
                    time_per_pixel,
                    visible,
                    points,
                    ctx.recording_engine().store(),
                    view_query,
                    label,
                    // Aggregation for points is not supported.
                    re_sdk_types::components::AggregationPolicy::Off,
                    component_identifier,
                    &mut series,
                );
            }
            Ok(series)
        }
    }
}
