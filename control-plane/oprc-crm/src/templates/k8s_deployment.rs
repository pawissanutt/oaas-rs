use super::manager::{
    EnvironmentContext, RenderContext, RenderedResource, Template, render_with,
};

#[derive(Clone, Debug, Default)]
pub struct K8sDeploymentTemplate;

impl Template for K8sDeploymentTemplate {
    fn name(&self) -> &'static str {
        "k8s_deployment"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["deployment", "k8s"]
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
        nfr: Option<&crate::crd::class_runtime::NfrRequirementsSpec>,
    ) -> i32 {
        // Prefer full when profile indicates full/prod or when running in datacenter
        // Give K8s deployment a high score for full profile/datacenter, but
        // make it lower than Knative's so Knative is preferred when available.
        let mut s = if env.profile.eq_ignore_ascii_case("k8s_deployment") {
            800_000
        } else if env.is_datacenter {
            600_000
        } else {
            0
        };
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
