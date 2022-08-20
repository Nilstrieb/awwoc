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
fn vec() {
    let mut vec = Vec::new();

    let len = if cfg!(miri) { 100 } else { 10_000 };

    for i in 0..len {
        vec.push(i);
    }

    assert!(vec.iter().enumerate().all(|(i, &item)| i == item));
}

#[test]
fn btree_map() {
    let mut map = BTreeMap::new();

    let len = if cfg!(miri) { 10 } else { 1000 };

    for i in (0..len).map(|i| i * 3) {
        map.insert(i, i + 10);
    }

    assert!(map.iter().all(|(k, v)| *v == *k + 10));
}
