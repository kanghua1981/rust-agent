//! 插件技能加载器
//! 
//! 从插件目录加载技能文档。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::PluginError;

/// 技能定义
#[derive(Debug, Clone)]
pub struct SkillDefinition {
    /// 技能名称
    pub name: String,
    /// 技能描述
    pub description: String,
    /// 技能内容（Markdown格式）
    pub content: String,
    /// 技能文件路径
    pub file_path: PathBuf,
    /// 所属插件
    pub plugin_id: String,
    /// 技能标签
    pub tags: Vec<String>,
}

/// 技能加载器
#[derive(Debug, Clone)]
pub struct SkillLoader {
    /// 已加载的技能（技能全名 -> 技能定义）
    loaded_skills: HashMap<String, SkillDefinition>,
    /// 插件技能映射（插件ID -> 技能名称列表）
    plugin_skills: HashMap<String, Vec<String>>,
    /// 技能标签索引（标签 -> 技能名称列表）
    tag_index: HashMap<String, Vec<String>>,
}

impl SkillLoader {
    /// 创建技能加载器
    pub fn new() -> Self {
        Self {
            loaded_skills: HashMap::new(),
            plugin_skills: HashMap::new(),
            tag_index: HashMap::new(),
        }
    }
    
    /// 从插件目录加载技能
    pub fn load_skills_from_plugin(&mut self, plugin_id: &str, plugin_path: &Path) -> Result<Vec<SkillDefinition>, PluginError> {
        let skills_dir = plugin_path.join("skills");
        
        // 检查技能目录是否存在
        if !skills_dir.exists() || !skills_dir.is_dir() {
            return Ok(Vec::new());
        }
        
        let mut skills = Vec::new();
        
        // 遍历技能目录
        let entries = std::fs::read_dir(&skills_dir)
            .map_err(|e| PluginError::Io(e))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| PluginError::Io(e))?;
            let path = entry.path();
            
            // 只处理Markdown文件
            if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
                match self.load_skill_from_markdown(&path, plugin_id) {
                    Ok(skill) => {
                        skills.push(skill.clone());
                        self.register_skill(skill, plugin_id)?;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load skill from {:?}: {}", path, e);
                    }
                }
            }
        }
        
        tracing::info!("Loaded {} skills from plugin {}", skills.len(), plugin_id);
        Ok(skills)
    }
    
    /// 从Markdown文件加载技能
    fn load_skill_from_markdown(&self, markdown_path: &Path, plugin_id: &str) -> Result<SkillDefinition, PluginError> {
        // 读取Markdown文件
        let content = std::fs::read_to_string(markdown_path)
            .map_err(|e| PluginError::Io(e))?;
        
        // 解析技能名称（从文件名或文件内容）
        let file_name = markdown_path.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        
        // 尝试从YAML frontmatter中提取元数据
        let (name, description, tags) = self.parse_frontmatter(&content, &file_name);
        
        Ok(SkillDefinition {
            name,
            description,
            content,
            file_path: markdown_path.to_path_buf(),
            plugin_id: plugin_id.to_string(),
            tags,
        })
    }
    
    /// 解析YAML frontmatter
    fn parse_frontmatter(&self, content: &str, default_name: &str) -> (String, String, Vec<String>) {
        let mut name = default_name.to_string();
        let mut description = String::new();
        let mut tags = Vec::new();
        
        // 检查是否有YAML frontmatter
        if content.starts_with("---\n") {
            if let Some(end_pos) = content.find("\n---\n") {
                let frontmatter = &content[4..end_pos];
                
                // 简单解析YAML
                for line in frontmatter.lines() {
                    if line.starts_with("name:") {
                        name = line[5..].trim().to_string();
                    } else if line.starts_with("description:") {
                        description = line[12..].trim().to_string();
                    } else if line.starts_with("tags:") {
                        let tags_str = line[5..].trim();
                        if tags_str.starts_with('[') && tags_str.ends_with(']') {
                            let tags_content = &tags_str[1..tags_str.len()-1];
                            tags = tags_content.split(',')
                                .map(|t| t.trim().trim_matches('"').trim_matches('\'').to_string())
                                .filter(|t| !t.is_empty())
                                .collect();
                        }
                    }
                }
            }
        }
        
        // 如果没有frontmatter，尝试从内容中提取描述
        if description.is_empty() {
            description = self.extract_description_from_content(content);
        }
        
        (name, description, tags)
    }
    
    /// 从内容中提取描述
    fn extract_description_from_content(&self, content: &str) -> String {
        // 取第一段非空行作为描述
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                // 限制描述长度
                if trimmed.len() > 200 {
                    return format!("{}...", &trimmed[..200]);
                }
                return trimmed.to_string();
            }
        }
        
        "No description".to_string()
    }
    
    /// 注册技能
    fn register_skill(&mut self, skill: SkillDefinition, plugin_id: &str) -> Result<(), PluginError> {
        let skill_full_name = format!("{}@{}", skill.name, plugin_id);
        
        // 检查技能是否已存在
        if self.loaded_skills.contains_key(&skill_full_name) {
            return Err(PluginError::Conflict(format!(
                "Skill {} already registered", skill_full_name
            )));
        }
        
        // 注册技能
        self.loaded_skills.insert(skill_full_name.clone(), skill.clone());
        
        // 更新插件技能映射
        self.plugin_skills
            .entry(plugin_id.to_string())
            .or_insert_with(Vec::new)
            .push(skill_full_name.clone());
        
        // 更新标签索引
        for tag in &skill.tags {
            self.tag_index
                .entry(tag.clone())
                .or_insert_with(Vec::new)
                .push(skill_full_name.clone());
        }
        
        Ok(())
    }
    
    /// 获取所有已加载的技能
    pub fn all_skills(&self) -> Vec<&SkillDefinition> {
        self.loaded_skills.values().collect()
    }
    
    /// 按名称获取技能
    pub fn get_skill(&self, skill_name: &str) -> Option<&SkillDefinition> {
        // 首先尝试精确匹配
        if let Some(skill) = self.loaded_skills.get(skill_name) {
            return Some(skill);
        }
        
        // 如果没有@符号，尝试模糊匹配
        if !skill_name.contains('@') {
            // 查找所有匹配的技能
            for (full_name, skill) in &self.loaded_skills {
                if full_name.starts_with(&format!("{}@", skill_name)) {
                    return Some(skill);
                }
            }
        }
        
        None
    }
    
    /// 获取插件的所有技能
    pub fn get_plugin_skills(&self, plugin_id: &str) -> Vec<&SkillDefinition> {
        let mut skills = Vec::new();
        
        if let Some(skill_names) = self.plugin_skills.get(plugin_id) {
            for skill_name in skill_names {
                if let Some(skill) = self.loaded_skills.get(skill_name) {
                    skills.push(skill);
                }
            }
        }
        
        skills
    }
    
    /// 按标签获取技能
    pub fn get_skills_by_tag(&self, tag: &str) -> Vec<&SkillDefinition> {
        let mut skills = Vec::new();
        
        if let Some(skill_names) = self.tag_index.get(tag) {
            for skill_name in skill_names {
                if let Some(skill) = self.loaded_skills.get(skill_name) {
                    skills.push(skill);
                }
            }
        }
        
        skills
    }
    
    /// 搜索技能
    pub fn search_skills(&self, query: &str) -> Vec<&SkillDefinition> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        
        for skill in self.loaded_skills.values() {
            if skill.name.to_lowercase().contains(&query_lower) ||
               skill.description.to_lowercase().contains(&query_lower) ||
               skill.content.to_lowercase().contains(&query_lower) ||
               skill.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
            {
                results.push(skill);
            }
        }
        
        // 按相关性排序（简单实现：名称匹配 > 描述匹配 > 内容匹配）
        results.sort_by(|a, b| {
            let a_score = self.calculate_relevance_score(a, &query_lower);
            let b_score = self.calculate_relevance_score(b, &query_lower);
            b_score.cmp(&a_score)
        });
        
        results
    }
    
    /// 计算技能相关性分数
    fn calculate_relevance_score(&self, skill: &SkillDefinition, query: &str) -> i32 {
        let mut score = 0;
        
        // 名称完全匹配：最高分
        if skill.name.to_lowercase() == query {
            score += 100;
        }
        // 名称包含查询：较高分
        else if skill.name.to_lowercase().contains(query) {
            score += 50;
        }
        
        // 描述包含查询：中等分
        if skill.description.to_lowercase().contains(query) {
            score += 20;
        }
        
        // 标签包含查询：中等分
        if skill.tags.iter().any(|tag| tag.to_lowercase().contains(query)) {
            score += 15;
        }
        
        // 内容包含查询：较低分
        if skill.content.to_lowercase().contains(query) {
            score += 5;
        }
        
        score
    }
    
    /// 获取所有标签
    pub fn all_tags(&self) -> Vec<&String> {
        self.tag_index.keys().collect()
    }
    
    /// 获取技能统计信息
    pub fn stats(&self) -> SkillStats {
        let mut stats = SkillStats::default();
        
        stats.total = self.loaded_skills.len();
        stats.plugins = self.plugin_skills.len();
        stats.tags = self.tag_index.len();
        
        // 计算平均技能长度
        let total_chars: usize = self.loaded_skills.values()
            .map(|s| s.content.len())
            .sum();
        
        if stats.total > 0 {
            stats.avg_length = total_chars / stats.total;
        }
        
        stats
    }
    
    /// 清除所有已加载的技能
    pub fn clear(&mut self) {
        self.loaded_skills.clear();
        self.plugin_skills.clear();
        self.tag_index.clear();
    }
}

/// 技能统计信息
#[derive(Debug, Clone, Default)]
pub struct SkillStats {
    /// 技能总数
    pub total: usize,
    /// 插件数量
    pub plugins: usize,
    /// 标签数量
    pub tags: usize,
    /// 平均技能长度（字符数）
    pub avg_length: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_skill_loader() {
        let loader = SkillLoader::new();
        
        // 测试初始状态
        assert_eq!(loader.all_skills().len(), 0);
        assert_eq!(loader.all_tags().len(), 0);
        
        // 测试获取不存在的技能
        assert!(loader.get_skill("nonexistent").is_none());
    }
    
    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: Git Commands
description: Common Git commands and workflows
tags: [git, version-control, commands]
---
# Git Commands

This skill covers common Git commands..."#;
        
        let loader = SkillLoader::new();
        let (name, description, tags) = loader.parse_frontmatter(content, "default");
        
        assert_eq!(name, "Git Commands");
        assert_eq!(description, "Common Git commands and workflows");
        assert_eq!(tags, vec!["git", "version-control", "commands"]);
    }
    
    #[test]
    fn test_extract_description_from_content() {
        let content = r#"# Git Commands

This skill covers common Git commands and workflows used in daily development.

## Basic Commands

- git status
- git add
- git commit"#;
        
        let loader = SkillLoader::new();
        let description = loader.extract_description_from_content(content);
        
        assert_eq!(description, "This skill covers common Git commands and workflows used in daily development.");
    }
    
    #[test]
    fn test_skill_stats() {
        let stats = SkillStats {
            total: 10,
            plugins: 3,
            tags: 5,
            avg_length: 1500,
        };
        
        assert_eq!(stats.total, 10);
        assert_eq!(stats.plugins, 3);
        assert_eq!(stats.tags, 5);
        assert_eq!(stats.avg_length, 1500);
    }
}