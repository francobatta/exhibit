//! The adapters compiled into Exhibit. A new service is one line in `all`.

use exhibit_core::service::Service;
use exhibit_letterboxd::Letterboxd;
use exhibit_mdblist::MdbList;
use exhibit_wikidata::Wikidata;

/// In the order Settings lists them: the owner's activity, then the public's verdict.
pub fn all() -> Vec<Box<dyn Service>> {
    vec![Box::new(Letterboxd), Box::new(MdbList), Box::new(Wikidata)]
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
        assert!(find("mdblist").is_some());
        assert!(find("wikidata").is_some());
        assert!(find("nope").is_none());
        let ids: Vec<_> = all().iter().map(|s| s.info().id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
    }

    #[test]
    fn origins_are_unique_across_services() {
        let mut ids: Vec<_> = all()
            .iter()
            .flat_map(|s| s.info().origins.iter().map(|o| o.id))
            .collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }
}
