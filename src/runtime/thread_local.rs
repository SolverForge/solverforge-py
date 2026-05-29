use std::cell::RefCell;
use std::sync::Arc;

use crate::schema::DynamicSchema;

thread_local! {
    static ACTIVE_SCHEMA: RefCell<Option<Arc<DynamicSchema>>> = const { RefCell::new(None) };
}

pub fn with_schema<T>(schema: Arc<DynamicSchema>, f: impl FnOnce() -> T) -> T {
    ACTIVE_SCHEMA.with(|slot| {
        *slot.borrow_mut() = Some(schema);
        let result = f();
        *slot.borrow_mut() = None;
        result
    })
}

pub fn active_schema() -> Option<Arc<DynamicSchema>> {
    ACTIVE_SCHEMA.with(|slot| slot.borrow().clone())
}
