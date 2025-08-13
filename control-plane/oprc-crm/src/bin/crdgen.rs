use kube::core::CustomResourceExt;
use oprc_crm::crd::deployment_record::DeploymentRecord;

fn main() {
    let crd = DeploymentRecord::crd();
    let yaml = serde_yaml::to_string(&crd).expect("serialize CRD to YAML");
    println!("{}", yaml);
}
