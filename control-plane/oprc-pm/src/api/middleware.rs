use tower::ServiceBuilder;
use tower_http::{
    classify::ServerErrorsAsFailures, classify::SharedClassifier,
    cors::CorsLayer, trace::TraceLayer,
};

// Simplified middleware stack
pub fn create_middleware_stack() -> ServiceBuilder<
    tower::layer::util::Stack<
        CorsLayer,
        tower::layer::util::Stack<
            TraceLayer<SharedClassifier<ServerErrorsAsFailures>>,
            tower::layer::util::Identity,
        >,
    >,
> {
    ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive()) // For development - should be more restrictive in production
}
