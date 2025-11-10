pub mod actions;
pub mod descriptor;
pub mod descriptors_builtin;
pub mod evaluator;
pub mod observed;
pub mod observed_lister;

pub mod function_readiness;
pub use actions::FsmAction;
pub use descriptor::{
    ManagedKind, ReadinessResult, ReadinessRule, TemplateDescriptor,
};
pub use observed::{ChildStatus, Observed};
pub use observed_lister::observe_children;
pub mod conditions;
pub use evaluator::{EvalInput, EvalOutput, Phase, evaluate};

// Unit tests for FSM evaluator live in a sibling module file
#[cfg(test)]
mod descriptors_tests;
#[cfg(test)]
mod evaluator_tests;
#[cfg(test)]
mod observed_lister_tests;
