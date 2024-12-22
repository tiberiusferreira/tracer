use crate::ui::trace::grid::SearchFor;
use crate::Endpoint;

pub struct TraceSummary;

impl Endpoint for TraceSummary {
    const PATH: &'static str = "/api/ui/trace/time_series";
    const METHOD: &'static str = "GET";
    type RequestBody = ();
    type QueryParameters = SearchFor;
    type ResponseBody = crate::ui::chart::TimeBucketChart;
}
