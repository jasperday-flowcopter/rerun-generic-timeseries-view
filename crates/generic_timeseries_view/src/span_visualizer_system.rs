use re_viewport_blueprint::ViewPropertyQueryError;
use rerun::external::{re_renderer, re_viewer_context::{self, IdentifiedViewSystem, ViewContext, ViewQuery, ViewSystemExecutionError, VisualizerQueryInfo, VisualizerSystem}};

pub struct SeriesSpanSystem {

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
        _ctx: &ViewContext<'_>,
        _query: &ViewQuery<'_>,
    ) -> Result<(), ViewPropertyQueryError> {
        todo!()
    }
}