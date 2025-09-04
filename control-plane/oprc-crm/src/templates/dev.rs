use super::manager::{
    EnvironmentContext, RenderContext, RenderedResource, Template, render_with,
};

#[derive(Clone, Debug, Default)]
pub struct DevTemplate;

impl Template for DevTemplate {
    fn name(&self) -> &'static str {
        "dev"
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
        _nfr: Option<&crate::crd::class_runtime::NfrRequirementsSpec>,
    ) -> i32 {
        // Strongly prefer when profile explicitly set to dev.
        if env.profile.eq_ignore_ascii_case("dev") {
            1_000_000
        } else if env.is_edge {
            // Dev templates are acceptable on edge but lower priority than edge-specific templates
            10
        } else {
            // Discourage selection in datacenter/full environments
            -10
        }
    }
}
