export interface LocalizedText {
  en: string;
  zh: string;
}

export interface FeaturedMarketplacesConfig {
  version: string;
  last_updated: string;
  marketplace: FeaturedMarketplaceCategory[];
}

export interface FeaturedMarketplaceCategory {
  id: string;
  name: LocalizedText;
  description: LocalizedText;
  marketplaces: FeaturedMarketplace[];
}

export interface FeaturedMarketplaceOwner {
  name?: string;
  email?: string;
  url?: string;
}

export interface FeaturedMarketplace {
  marketplace_name: string;
  marketplace_repo: string;
  repository_url?: string;
  marketplace_add_command?: string;
  description: LocalizedText;
  owner?: FeaturedMarketplaceOwner;
  tags: string[];
  featured: boolean;
  plugins: FeaturedMarketplacePlugin[];
}

export interface FeaturedMarketplacePlugin {
  name: string;
  install_command?: string;
  description: LocalizedText;
  version?: string;
  author?: FeaturedMarketplaceOwner;
  source?: string;
  tags: string[];
  featured: boolean;
}
