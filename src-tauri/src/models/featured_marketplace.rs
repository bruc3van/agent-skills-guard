use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalizedText {
    pub en: String,
    pub zh: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturedMarketplacesConfig {
    pub version: String,
    pub last_updated: String,
    #[serde(rename = "marketplace", alias = "categories")]
    pub marketplace: Vec<FeaturedMarketplaceCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturedMarketplaceCategory {
    pub id: String,
    pub name: LocalizedText,
    pub description: LocalizedText,
    pub marketplaces: Vec<FeaturedMarketplace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturedMarketplaceOwner {
    pub name: Option<String>,
    pub email: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturedMarketplace {
    pub marketplace_name: String,
    pub marketplace_repo: String,
    pub repository_url: Option<String>,
    pub marketplace_add_command: Option<String>,
    pub description: LocalizedText,
    pub owner: Option<FeaturedMarketplaceOwner>,
    pub tags: Vec<String>,
    pub featured: bool,
    pub plugins: Vec<FeaturedMarketplacePlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturedMarketplacePlugin {
    pub name: String,
    pub install_command: Option<String>,
    pub description: LocalizedText,
    pub version: Option<String>,
    pub author: Option<FeaturedMarketplaceOwner>,
    pub source: Option<String>,
    pub tags: Vec<String>,
    pub featured: bool,
}
