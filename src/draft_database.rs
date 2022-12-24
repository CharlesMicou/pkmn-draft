use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::fs;

use crate::draft_engine::DraftItemId;

pub struct DraftItem {
    foo: String,
    // todo templating here
}

impl DraftItem {
    pub fn get_template(&self) -> &String {
        return &self.foo;
    }
}

pub struct DraftDatabase {
    id_list: Vec<DraftItemId>,
    id_to_item: HashMap<DraftItemId, DraftItem>,
}

impl DraftDatabase {
    pub fn from_folder(dir_name: &str) -> io::Result<DraftDatabase> {
        let path = Path::new(dir_name);
        let mut items: HashMap<DraftItemId, DraftItem> = HashMap::new();
        let mut i = 0;
        for file in fs::read_dir(path)? {
            //println!("{}", file.unwrap().path().display());
            items.insert(i, DraftItem{foo: format!("dummy item {i}")});
            i += 1;
        };
        let id_list: Vec<DraftItemId> = items.keys().copied().collect();
        Ok(DraftDatabase {
            id_list,
            id_to_item: items,
        })
    }

    pub fn get_item_list(&self) -> &Vec<DraftItemId> {
        return &self.id_list;
    }

    pub fn get_item_by_id(&self, id: &DraftItemId) -> Option<&DraftItem> {
        self.id_to_item.get(id)
    }
}