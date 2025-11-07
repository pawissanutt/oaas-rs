use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use oprc_odgm::identity::normalize_object_id;
use oprc_odgm::storage_key::{
    string_object_entry_key_string, string_object_meta_key,
};

fn bench_key_encoding(c: &mut Criterion) {
    let id = "order-abc-1234567890";
    let norm = normalize_object_id(id, 160).unwrap();
    c.bench_function("string_meta_key", |b| {
        b.iter(|| {
            let k = string_object_meta_key(black_box(&norm));
            black_box(k);
        })
    });
    c.bench_function("string_entry_key_short", |b| {
        b.iter(|| {
            let k = string_object_entry_key_string(
                black_box(&norm),
                black_box("f"),
            );
            black_box(k);
        })
    });
    c.bench_function("string_entry_key_long", |b| {
        b.iter(|| {
            let k = string_object_entry_key_string(
                black_box(&norm),
                black_box("field-long-name-XYZ"),
            );
            black_box(k);
        })
    });
}

criterion_group!(benches, bench_key_encoding);
criterion_main!(benches);
