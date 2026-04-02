#![allow(dead_code)]

use super::profile::BrowserProfile;
use crate::tools::browser::error::{BrowserError, configuration_error};
use serde_json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Manages browser configuration profiles
pub struct ConfigManager {
    /// Directory where profiles are stored
    profiles_dir: PathBuf,
    /// Loaded profiles
    profiles: HashMap<String, BrowserProfile>,
    /// Default profile name
    default_profile: String,
}

impl ConfigManager {
    /// Create a new config manager
    pub fn new(profiles_dir: PathBuf) -> Result<Self, BrowserError> {
        let mut manager = Self {
            profiles_dir,
            profiles: HashMap::new(),
            default_profile: "default".to_string(),
        };
        
        manager.load_profiles()?;
        Ok(manager)
    }
    
    /// Load all profiles from the profiles directory
    fn load_profiles(&mut self) -> Result<(), BrowserError> {
        // Create directory if it doesn't exist
        if !self.profiles_dir.exists() {
            fs::create_dir_all(&self.profiles_dir)
                .map_err(|e| configuration_error(format!("Failed to create profiles directory: {}", e)))?;
        }
        
        // Load all .json files in the directory
        let entries = fs::read_dir(&self.profiles_dir)
            .map_err(|e| configuration_error(format!("Failed to read profiles directory: {}", e)))?;
        
        for entry in entries {
            let entry = entry
                .map_err(|e| configuration_error(format!("Failed to read directory entry: {}", e)))?;
            
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                if let Some(file_name) = path.file_stem().and_then(|name| name.to_str()) {
                    match self.load_profile_from_file(&path) {
                        Ok(profile) => {
                            self.profiles.insert(file_name.to_string(), profile);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load profile from {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
        
        // Ensure default profile exists
        if !self.profiles.contains_key(&self.default_profile) {
            let default_profile = BrowserProfile::default();
            self.profiles.insert(self.default_profile.clone(), default_profile);
            self.save_profile(&self.default_profile)?;
        }
        
        Ok(())
    }
    
    /// Load a profile from a JSON file
    fn load_profile_from_file(&self, path: &Path) -> Result<BrowserProfile, BrowserError> {
        let content = fs::read_to_string(path)
            .map_err(|e| configuration_error(format!("Failed to read profile file {}: {}", path.display(), e)))?;
        
        let profile: BrowserProfile = serde_json::from_str(&content)
            .map_err(|e| configuration_error(format!("Failed to parse profile JSON from {}: {}", path.display(), e)))?;
        
        profile.validate()
            .map_err(|e| configuration_error(format!("Invalid profile in {}: {}", path.display(), e)))?;
        
        Ok(profile)
    }
    
    /// Save a profile to a JSON file
    fn save_profile(&self, profile_name: &str) -> Result<(), BrowserError> {
        let profile = self.profiles.get(profile_name)
            .ok_or_else(|| configuration_error(format!("Profile '{}' not found", profile_name)))?;
        
        let file_path = self.profiles_dir.join(format!("{}.json", profile_name));
        let json = serde_json::to_string_pretty(profile)
            .map_err(|e| configuration_error(format!("Failed to serialize profile: {}", e)))?;
        
        fs::write(&file_path, json)
            .map_err(|e| configuration_error(format!("Failed to write profile to {}: {}", file_path.display(), e)))?;
        
        Ok(())
    }
    
    /// Get a profile by name
    pub fn get_profile(&self, name: &str) -> Option<&BrowserProfile> {
        self.profiles.get(name)
    }
    
    /// Get the default profile
    pub fn get_default_profile(&self) -> &BrowserProfile {
        self.profiles.get(&self.default_profile)
            .expect("Default profile should always exist")
    }
    
    /// Create a new profile
    pub fn create_profile(&mut self, profile: BrowserProfile) -> Result<(), BrowserError> {
        profile.validate()
            .map_err(|e| configuration_error(format!("Invalid profile: {}", e)))?;
        
        let name = profile.name.clone();
        self.profiles.insert(name.clone(), profile);
        self.save_profile(&name)?;
        
        Ok(())
    }
    
    /// Update an existing profile
    pub fn update_profile(&mut self, name: &str, profile: BrowserProfile) -> Result<(), BrowserError> {
        if !self.profiles.contains_key(name) {
            return Err(configuration_error(format!("Profile '{}' not found", name)));
        }
        
        profile.validate()
            .map_err(|e| configuration_error(format!("Invalid profile: {}", e)))?;
        
        self.profiles.insert(name.to_string(), profile);
        self.save_profile(name)?;
        
        Ok(())
    }
    
    /// Delete a profile
    pub fn delete_profile(&mut self, name: &str) -> Result<(), BrowserError> {
        if name == self.default_profile {
            return Err(configuration_error("Cannot delete default profile".to_string()));
        }
        
        if !self.profiles.contains_key(name) {
            return Err(configuration_error(format!("Profile '{}' not found", name)));
        }
        
        self.profiles.remove(name);
        
        let file_path = self.profiles_dir.join(format!("{}.json", name));
        if file_path.exists() {
            fs::remove_file(&file_path)
                .map_err(|e| configuration_error(format!("Failed to delete profile file {}: {}", file_path.display(), e)))?;
        }
        
        Ok(())
    }
    
    /// List all available profiles
    pub fn list_profiles(&self) -> Vec<&BrowserProfile> {
        self.profiles.values().collect()
    }
    
    /// Get profile names
    pub fn profile_names(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }
    
    /// Set the default profile
    pub fn set_default_profile(&mut self, name: &str) -> Result<(), BrowserError> {
        if !self.profiles.contains_key(name) {
            return Err(configuration_error(format!("Profile '{}' not found", name)));
        }
        
        self.default_profile = name.to_string();
        Ok(())
    }
    
    /// Get the default profile name
    pub fn default_profile_name(&self) -> &str {
        &self.default_profile
    }
    
    /// Get the profiles directory
    pub fn profiles_dir(&self) -> &Path {
        &self.profiles_dir
    }
}