use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::draft_engine::DraftItemId;

pub struct DraftItem {
    raw_html: String,
    simple_text: String,
}

impl DraftItem {
    pub fn get_template(&self) -> &String {
        return &self.raw_html;
    }
    pub fn get_raw(&self) -> &String {
        return &self.simple_text;
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
        for entry in fs::read_dir(path)? {
            let entry_path = entry?.path();
            let raw_html = fs::read_to_string(&entry_path)?;
            let simple_text = html2text::from_read(fs::File::open(&entry_path)?, 999);
            items.insert(i, DraftItem{ raw_html , simple_text});
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