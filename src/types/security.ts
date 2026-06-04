export interface SecurityIssue {
  severity: string;
  category: string;
  description: string;
  line_number?: number;
  code_snippet?: string;
  file_path?: string; // 记录哪个文件有风险
  rule_id?: string;
  confidence?: string;
  remediation?: string;
  cwe_id?: string;
  threat_category?: string;
  same_path_other_rule_ids?: string[];
  finding_kind?: string; // Security | Auditability | Structure
}

/// 各 FindingKind 的数量统计
export interface KindCounts {
  security: number;
  auditability: number;
  structure: number;
}

export interface SecurityReport {
  skill_id: string;
  score: number;
  level: string;
  issues: SecurityIssue[];
  recommendations: string[];
  blocked: boolean;
  hard_trigger_issues: string[];
  scanned_files: string[]; // 已扫描的文件列表
  partial_scan: boolean; // 是否存在未完整扫描
  skipped_files: string[]; // 跳过扫描的文件列表
  kind_counts?: KindCounts; // 各 kind 的数量统计
}

export interface SkillScanResult {
  skill_id: string;
  skill_name: string;
  score: number;
  level: string;
  scanned_at: string;
  report: SecurityReport;
}
