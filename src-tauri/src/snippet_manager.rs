use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetItem {
    pub id: String,
    pub title: String,
    pub content_type: String, // "text" or "image"
    pub content: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub sort_order: u32,
}

fn default_category() -> String {
    "General".to_string()
}

pub struct SnippetManager {
    file_path: PathBuf,
    snippets: Mutex<Vec<SnippetItem>>,
}

impl SnippetManager {
    pub fn new() -> Result<Self, String> {
        let app_dir = dirs::data_local_dir()
            .ok_or_else(|| "Cannot find app data dir".to_string())?
            .join("screenshot-tool");
            
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;

        let file_path = app_dir.join("snippets.json");
        let snippets = if file_path.exists() {
            match fs::read_to_string(&file_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| Vec::new()),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        Ok(Self {
            file_path,
            snippets: Mutex::new(snippets),
        })
    }

    pub fn get_all(&self) -> Vec<SnippetItem> {
        let guard = self.snippets.lock().unwrap();
        let mut items = guard.clone();
        items.sort_by(|a, b| a.category.cmp(&b.category).then(a.sort_order.cmp(&b.sort_order)));
        items
    }

    pub fn get_categories(&self) -> Vec<String> {
        let guard = self.snippets.lock().unwrap();
        let mut cats: Vec<String> = guard.iter().map(|s| s.category.clone()).collect();
        cats.sort();
        cats.dedup();
        if cats.is_empty() {
            cats.push("General".to_string());
        }
        cats
    }

    pub fn add_with_category(&self, title: String, content_type: String, content: String, category: String) -> SnippetItem {
        let sort_order = {
            let guard = self.snippets.lock().unwrap();
            guard.iter().filter(|s| s.category == category).map(|s| s.sort_order).max().unwrap_or(0) + 1
        };
        let new_item = SnippetItem {
            id: Uuid::new_v4().to_string(),
            title,
            content_type,
            content,
            category,
            sort_order,
        };

        {
            let mut guard = self.snippets.lock().unwrap();
            guard.push(new_item.clone());
        }

        self.save();
        new_item
    }

    pub fn add(&self, title: String, content_type: String, content: String) -> SnippetItem {
        let sort_order = {
            let guard = self.snippets.lock().unwrap();
            guard.iter().filter(|s| s.category == "General").map(|s| s.sort_order).max().unwrap_or(0) + 1
        };
        let new_item = SnippetItem {
            id: Uuid::new_v4().to_string(),
            title,
            content_type,
            content,
            category: "General".to_string(),
            sort_order,
        };

        {
            let mut guard = self.snippets.lock().unwrap();
            guard.push(new_item.clone());
        }

        self.save();
        new_item
    }

    pub fn delete(&self, id: &str) -> bool {
        let mut guard = self.snippets.lock().unwrap();
        let initial_len = guard.len();
        guard.retain(|item| item.id != id);
        
        let changed = guard.len() < initial_len;
        if changed {
            drop(guard);
            self.save();
        }
        changed
    }

    pub fn update(&self, id: &str, title: String, content: String) -> bool {
        let mut guard = self.snippets.lock().unwrap();
        if let Some(item) = guard.iter_mut().find(|i| i.id == id) {
            item.title = title;
            item.content = content;
            drop(guard);
            self.save();
            true
        } else {
            false
        }
    }

    pub fn update_category(&self, id: &str, category: String) -> bool {
        let mut guard = self.snippets.lock().unwrap();
        if let Some(item) = guard.iter_mut().find(|i| i.id == id) {
            item.category = category;
            drop(guard);
            self.save();
            true
        } else {
            false
        }
    }

    pub fn reorder(&self, ordered_ids: Vec<String>) -> bool {
        let mut guard = self.snippets.lock().unwrap();
        for (i, id) in ordered_ids.iter().enumerate() {
            if let Some(item) = guard.iter_mut().find(|s| &s.id == id) {
                item.sort_order = i as u32;
            }
        }
        drop(guard);
        self.save();
        true
    }

    pub fn rename_category(&self, old_name: &str, new_name: &str) -> bool {
        let mut guard = self.snippets.lock().unwrap();
        let mut changed = false;
        for item in guard.iter_mut() {
            if item.category == old_name {
                item.category = new_name.to_string();
                changed = true;
            }
        }
        if changed {
            drop(guard);
            self.save();
        }
        changed
    }

    pub fn delete_category(&self, name: &str) {
        let mut guard = self.snippets.lock().unwrap();
        for item in guard.iter_mut() {
            if item.category == name {
                item.category = "General".to_string();
            }
        }
        drop(guard);
        self.save();
    }

    pub fn save(&self) {
        if let Ok(guard) = self.snippets.lock() {
            if let Ok(json) = serde_json::to_string_pretty(&*guard) {
                let _ = fs::write(&self.file_path, json);
            }
        }
    }
}
