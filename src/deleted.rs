use std::path::PathBuf;

pub struct TrashEntry {
    pub name: String,
    pub original_path: PathBuf,
    item: trash::TrashItem,
}

pub fn list_trash() -> Vec<TrashEntry> {
    let mut items = trash::os_limited::list().unwrap_or_default();
    items.sort_by_key(|a| std::cmp::Reverse(a.time_deleted));
    items
        .into_iter()
        .map(|item| TrashEntry {
            name: item.name.to_string_lossy().into_owned(),
            original_path: item.original_path(),
            item,
        })
        .collect()
}

pub fn restore(entry: &TrashEntry) -> Result<(), String> {
    trash::os_limited::restore_all([entry.item.clone()]).map_err(|e| e.to_string())
}

pub fn empty_all() -> Result<(), String> {
    let items = trash::os_limited::list().unwrap_or_default();
    trash::os_limited::purge_all(items).map_err(|e| e.to_string())
}
