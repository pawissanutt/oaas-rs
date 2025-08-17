use super::manager::{
    EnvironmentContext, RenderContext, RenderedResource, Template, render_with,
};

#[derive(Clone, Debug, Default)]
pub struct FullTemplate;

impl Template for FullTemplate {
    fn name(&self) -> &'static str {
        "full"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["prod", "production"]
    }
    fn render(&self, ctx: &RenderContext<'_>) -> Vec<RenderedResource> {
        let odgm_img_override = Some("ghcr.io/pawissanutt/oaas-rs/odgm:latest");
        render_with(ctx, 3, 1, odgm_img_override, None)
    }
    fn score(
        &self,
        _env: &EnvironmentContext<'_>,
        nfr: Option<&crate::crd::deployment_record::NfrRequirementsSpec>,
    ) -> i32 {
        let mut s = 0;
        if let Some(n) = nfr {
            if n.min_throughput_rps.unwrap_or(0) >= 1500 {
                s += 3;
            }
            if n.max_latency_ms.unwrap_or(u32::MAX) <= 50 {
                s += 3;
            }
            if n.availability_pct.unwrap_or(99.0) >= 99.95 {
                s += 2;
            }
            if n.consistency.as_deref() == Some("strong") {
                s += 2;
            }
        }
        s
    }
}
