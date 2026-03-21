use std::collections::HashMap;
use std::sync::Mutex;

static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);

struct Registry {
    docs: HashMap<u32, wcl::Document>,
    next_id: u32,
}

fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&mut Registry) -> R,
{
    let mut guard = REGISTRY.lock().unwrap();
    let reg = guard.get_or_insert_with(|| Registry {
        docs: HashMap::new(),
        next_id: 1,
    });
    f(reg)
}

pub fn store(doc: wcl::Document) -> u32 {
    with_registry(|reg| {
        let id = reg.next_id;
        reg.next_id = reg.next_id.wrapping_add(1);
        if reg.next_id == 0 {
            reg.next_id = 1;
        }
        reg.docs.insert(id, doc);
        id
    })
}

pub fn with<F, R>(handle: u32, f: F) -> Option<R>
where
    F: FnOnce(&wcl::Document) -> R,
{
    with_registry(|reg| reg.docs.get(&handle).map(f))
}

pub fn remove(handle: u32) {
    with_registry(|reg| {
        reg.docs.remove(&handle);
    });
}
