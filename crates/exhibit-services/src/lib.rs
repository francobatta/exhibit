//! The adapters compiled into Exhibit. A new service is one line in `all`.

use exhibit_core::service::Service;
use exhibit_letterboxd::Letterboxd;

pub fn all() -> Vec<Box<dyn Service>> {
    vec![Box::new(Letterboxd)]
}

pub fn find(id: &str) -> Option<Box<dyn Service>> {
    all().into_iter().find(|s| s.info().id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_known() {
        assert!(find("letterboxd").is_some());
        assert!(find("nope").is_none());
        let ids: Vec<_> = all().iter().map(|s| s.info().id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
    }
}
