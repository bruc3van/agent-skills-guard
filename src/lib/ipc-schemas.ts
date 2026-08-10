import { z } from "zod";

export const securityIssueSchema = z
  .object({
    severity: z.string(),
    category: z.string(),
    description: z.string(),
    line_number: z.number().int().nonnegative().nullish(),
    code_snippet: z.string().nullish(),
    file_path: z.string().nullish(),
    rule_id: z.string().optional(),
  })
  .passthrough();

export const securityReportSchema = z
  .object({
    skill_id: z.string(),
    score: z.number().finite(),
    level: z.string(),
    issues: z.array(securityIssueSchema),
    recommendations: z.array(z.string()),
    blocked: z.boolean(),
    hard_trigger_issues: z.array(z.string()),
    scanned_files: z.array(z.string()).default([]),
    partial_scan: z.boolean(),
    skipped_files: z.array(z.string()),
  })
  .passthrough();

export const skillScanResultSchema = z
  .object({
    skill_id: z.string(),
    skill_name: z.string(),
    score: z.number().finite(),
    level: z.string(),
    scanned_at: z.string(),
    report: securityReportSchema,
  })
  .passthrough();

export const skillUpdatePreparationSchema = z.tuple([securityReportSchema, z.array(z.string())]);
