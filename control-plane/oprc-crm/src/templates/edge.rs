use super::manager::{
    EnvironmentContext, RenderContext, RenderedResource, Template, render_with,
};

#[derive(Clone, Debug, Default)]
pub struct EdgeTemplate;

impl Template for EdgeTemplate {
    fn name(&self) -> &'static str {
        "edge"
    }
    fn render(
        &self,
        ctx: &RenderContext<'_>,
    ) -> Result<Vec<RenderedResource>, super::TemplateError> {
        let odgm_img_override = Some("ghcr.io/pawissanutt/oaas-rs/odgm:latest");
        render_with(ctx, 1, odgm_img_override, None)
    }
    fn score(
        &self,
        env: &EnvironmentContext<'_>,
        nfr: Option<&crate::crd::deployment_record::NfrRequirementsSpec>,
    ) -> i32 {
        // Prefer when profile is edge or environment reports edge
        let mut s = if env.profile.eq_ignore_ascii_case("edge") || env.is_edge {
            1_000_000
        } else {
            0
        };
        if let Some(n) = nfr {
            if n.min_throughput_rps.unwrap_or(0) >= 300 {
                s += 2;
            }
            if n.max_latency_ms.unwrap_or(u32::MAX) <= 150 {
                s += 2;
            }
            if n.availability_pct.unwrap_or(99.0) >= 99.9 {
                s += 1;
            }
        }
        s
    }
}
