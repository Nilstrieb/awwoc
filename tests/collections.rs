use std::collections::BTreeMap;

use awwoc::Awwoc;

#[global_allocator]
static AWWOC: Awwoc = Awwoc;

#[test]
fn vec() {
    let mut vec = Vec::new();

    for i in 0..10_000 {
        vec.push(i);
    }

    assert!(vec.iter().enumerate().all(|(i, &item)| i == item));
}

#[test]
fn btree_map() {
    let mut map = BTreeMap::new();

    for i in (0..1000).map(|i| i * 3) {
        map.insert(i, i + 10);
    }

    assert!(map.iter().all(|(k, v)| *v == *k + 10));
}
