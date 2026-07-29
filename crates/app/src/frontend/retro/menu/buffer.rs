#[derive(Debug, Clone, Default)]
pub struct MenuBuffer {
    items: Vec<String>,
    refs: Vec<*const str>,
}

impl MenuBuffer {
    pub fn add(&mut self, item: impl Into<String>) {
        let item = item.into();
        let ptr: *const str = item.as_str();
        self.refs.push(ptr);
        self.items.push(item);
    }

    pub fn get(&self) -> &[&str] {
        // SAFETY:
        // all pointers in `self.refs` point to valid strings in `self.items`
        // Convert &[ *const str ] -> &[ &str ] by transmuting each element
        unsafe { std::slice::from_raw_parts(self.refs.as_ptr() as *const &str, self.refs.len()) }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.refs.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
