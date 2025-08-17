use super::manager::{
    EnvironmentContext, RenderContext, RenderedResource, Template, render_with,
};

#[derive(Clone, Debug, Default)]
pub struct DevTemplate;

impl Template for DevTemplate {
    fn name(&self) -> &'static str {
        "dev"
    }
    fn render(&self, ctx: &RenderContext<'_>) -> Vec<RenderedResource> {
        let odgm_img_override = Some("ghcr.io/pawissanutt/oaas/odgm:latest");
        render_with(ctx, 1, 1, odgm_img_override, None)
    }
    fn score(
        &self,
        env: &EnvironmentContext<'_>,
        _nfr: Option<&crate::crd::deployment_record::NfrRequirementsSpec>,
    ) -> i32 {
        if env.profile.eq_ignore_ascii_case("dev") {
            1_000_000
        } else {
            0
        }
    }
}
