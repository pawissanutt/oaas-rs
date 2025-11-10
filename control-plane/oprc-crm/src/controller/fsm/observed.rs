use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct ChildStatus {
    pub name: String,
    pub kind: String,
    pub ready: bool,
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Observed {
    pub children: Vec<ChildStatus>,
}

impl Observed {
    pub fn readiness_by_kind(&self) -> HashMap<String, Vec<&ChildStatus>> {
        let mut map: HashMap<String, Vec<&ChildStatus>> = HashMap::new();
        for c in &self.children {
            map.entry(c.kind.clone()).or_default().push(c);
        }
        map
    }
}
