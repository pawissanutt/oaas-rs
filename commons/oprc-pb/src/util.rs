impl crate::ObjData {
    #[cfg(feature = "util")]
    pub fn pretty_print(&self) {
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
            let s = String::from_utf8_lossy(&v.data);
            println!("  {}: {}", k, s);
        }
        println!("}}");
    }

    #[cfg(all(feature = "util", not(feature = "bytes")))]
    pub fn get_owned_entry(&self, key: u32) -> Option<Vec<u8>> {
        self.entries.get(&key).map(|v| v.data.to_owned())
    }

    #[cfg(all(feature = "util", feature = "bytes"))]
    pub fn get_owned_entry(&self, key: u32) -> Option<bytes::Bytes> {
        self.entries.get(&key).map(|v| v.data.clone())
    }
}
