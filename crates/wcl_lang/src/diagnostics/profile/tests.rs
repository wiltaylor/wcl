use super::*;
use std::thread::sleep;

#[test]
fn enter_exit_records_one_invocation() {
    let cell = ProfileState::new_root();
    {
        let mut s = cell.lock().unwrap();
        s.enter(ProfileKey::Field { path: "a".into() });
    }
    sleep(Duration::from_millis(1));
    {
        let mut s = cell.lock().unwrap();
        s.exit();
    }
    let snap = cell.lock().unwrap().snapshot();
    let child = snap
        .root
        .children
        .get(&ProfileKey::Field { path: "a".into() });
    let child = child.expect("one entry under root");
    assert_eq!(child.count, 1);
    assert!(
        child.total >= Duration::from_micros(900),
        "{:?}",
        child.total
    );
    assert_eq!(child.min, child.max);
}

#[test]
fn nested_calls_form_tree() {
    let cell = ProfileState::new_root();
    {
        let mut s = cell.lock().unwrap();
        s.enter(ProfileKey::Field {
            path: "outer".into(),
        });
        s.enter(ProfileKey::Builtin { name: "map".into() });
        s.enter(ProfileKey::UserFn { name: "".into() });
        s.exit(); // userfn
        s.enter(ProfileKey::UserFn { name: "".into() });
        s.exit(); // userfn (aggregates with prior sibling)
        s.exit(); // map
        s.exit(); // outer
    }
    let snap = cell.lock().unwrap().snapshot();
    let outer = snap
        .root
        .children
        .get(&ProfileKey::Field {
            path: "outer".into(),
        })
        .unwrap();
    let map_node = outer
        .children
        .get(&ProfileKey::Builtin { name: "map".into() })
        .unwrap();
    let fn_node = map_node
        .children
        .get(&ProfileKey::UserFn { name: "".into() })
        .unwrap();
    assert_eq!(fn_node.count, 2);
    assert!(fn_node.total >= fn_node.max);
    assert!(fn_node.min <= fn_node.max);
}

#[test]
fn mean_is_zero_when_no_calls() {
    let n = ProfileNode::new(ProfileKey::Root);
    assert_eq!(n.mean(), Duration::ZERO);
}
