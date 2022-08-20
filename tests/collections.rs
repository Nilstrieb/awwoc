use std::collections::BTreeMap;

use awwoc::Awwoc;

#[global_allocator]
static AWWOC: Awwoc = Awwoc;

#[test]
fn boxed() {
    let mut boxed = Box::new(5);

    *boxed = 6;
    
    assert_eq!(*boxed, 6);
}

#[test]
#[ignore]
fn vec() {
    let mut vec = Vec::new();

    for i in 0..10_000 {
        vec.push(i);
    }

    assert!(vec.iter().enumerate().all(|(i, &item)| i == item));
}

#[test]
#[ignore]
fn btree_map() {
    let mut map = BTreeMap::new();

    for i in (0..1000).map(|i| i * 3) {
        map.insert(i, i + 10);
    }

    assert!(map.iter().all(|(k, v)| *v == *k + 10));
}
