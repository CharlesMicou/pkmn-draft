use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use rand::seq::SliceRandom;

use crate::draft_engine::DraftItemId;

pub struct DraftItem {
    raw_html: String,
    simple_text: String,
    stats_html: String,
}

impl DraftItem {
    pub fn get_template(&self) -> &String {
        return &self.raw_html;
    }
    pub fn get_raw(&self) -> &String {
        return &self.simple_text;
    }
    pub fn get_stats(&self) -> &String {
        return &self.stats_html;
    }
}

pub struct DraftSet {
    deduplicated_ids: HashMap<String, Vec<DraftItemId>>,
    id_to_item: HashMap<DraftItemId, DraftItem>,
}

impl DraftSet {
    pub fn from_folder(dir_name: &str) -> io::Result<DraftSet> {
        let dir: String = dir_name.to_string() + &*"/generated".to_string();
        let template_path = Path::new(&dir);
        let mut items: HashMap<DraftItemId, DraftItem> = HashMap::new();
        let mut deduplicated_ids: HashMap<String, Vec<DraftItemId>> = HashMap::new();
        let mut i = 0;
        for entry in fs::read_dir(template_path)? {
            let entry_path = entry?.path();
            let raw_html = fs::read_to_string(&entry_path)?;
            let simple_text = html2text::from_read(fs::File::open(&entry_path)?, 999).trim_start().to_string();

            let tmp: Vec<&str> = simple_text.split("@").collect();
            let pkmn_name = tmp[0].trim().to_string();
            if deduplicated_ids.contains_key(&pkmn_name) {
                deduplicated_ids.get_mut(&pkmn_name).unwrap().push(i);
            } else {
                deduplicated_ids.insert(pkmn_name, vec!(i));
            }

            let stats_file = dir_name.to_string() + &*"/generated_stats/".to_string() + &entry_path.file_name().unwrap().to_str().unwrap().to_string();
            let stats_html = fs::read_to_string(stats_file)?;
            items.insert(i, DraftItem{ raw_html , simple_text, stats_html});
            i += 1;
        };

        Ok(DraftSet {
            deduplicated_ids,
            id_to_item: items,
        })
    }

    pub fn get_item_list(&self) -> Vec<DraftItemId> {
        self.deduplicated_ids.values()
            .map(|variants| variants.choose(&mut rand::thread_rng()).unwrap().clone())
            .collect()
    }

    pub fn get_item_by_id(&self, id: &DraftItemId) -> Option<&DraftItem> {
        self.id_to_item.get(id)
    }
}

pub struct DraftDb {
    sets: HashMap<String, DraftSet>,
}

impl DraftDb {
    pub fn from_folder(dir_name: &str) -> io::Result<DraftDb> {
        let mut sets = HashMap::new();

        for entry in fs::read_dir(Path::new(dir_name))? {
            let entry_path = entry?.path();
            let set_name = entry_path.file_name().unwrap().to_str().unwrap().to_string();
            log::info!("Loading draft set {set_name}");
            let set = DraftSet::from_folder(entry_path.to_str().unwrap());
            match set {
                Ok(s) => {sets.insert(set_name, s);}
                Err(e) => log::error!("Failed to load {set_name}: {e}")
            }
        }

        Ok(DraftDb{
            sets,
        })
    }

    pub fn get_set(&self, set_name: &String) -> Option<&DraftSet> {
        self.sets.get(set_name)
    }
}