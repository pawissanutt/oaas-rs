use kube::core::CustomResourceExt;
use oprc_crm::crd::class_runtime::ClassRuntime;

fn main() {
    let crd = ClassRuntime::crd();
    let yaml = serde_yaml::to_string(&crd).expect("serialize CRD to YAML");
    println!("{}", yaml);
}
