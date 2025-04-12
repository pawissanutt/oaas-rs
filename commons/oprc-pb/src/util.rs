impl crate::ObjData {
    #[cfg(feature = "util")]
    pub fn pretty_print(&self) {
        use crate::val_data::Data;

        println!("{{");
        if let Some(metadata) = &self.metadata {
            println!(
            "  meta: {{cls_id:\"{}\", partition_id:\"{}\", object_id:\"{}\"}},",
            metadata.cls_id, metadata.partition_id, metadata.object_id
        );
        } else {
            println!("\tmeta: NONE");
        }
        let entries = &self.entries;
        for (k, v) in entries.iter() {
            match &v.data {
                Some(Data::Byte(b)) => {
                    let s = String::from_utf8_lossy(&b);
                    println!("  {}: {}", k, s);
                }
                Some(Data::CrdtMap(b)) => {
                    let doc = automerge::AutoCommit::load(&b[..]).unwrap();
                    let serialized = serde_json::to_string(
                        &automerge::AutoSerde::from(&doc),
                    )
                    .unwrap();
                    println!("  {}: {}", k, serialized);
                }
                None => {
                    println!("  {}: null", k);
                }
            }
        }
        println!("}}");
    }

    #[cfg(feature = "util")]
    pub fn get_owned_entry(&self, key: u32) -> Option<Vec<u8>> {
        if let Some(v) = self.entries.get(&key) {
            if let Some(data) = &v.data {
                match data {
                    crate::val_data::Data::Byte(items) => {
                        return Some(items.to_owned());
                    }
                    crate::val_data::Data::CrdtMap(items) => {
                        return Some(items.to_owned());
                    }
                }
            }
        }
        return None;
    }
}
