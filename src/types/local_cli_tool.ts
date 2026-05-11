export interface LocalCliTool {
  id: string;
  detected_path: string;
  manager: string;
  current_version?: string;
  latest_version?: string;
  update_available: boolean;
  last_checked?: string;
  update_status?: string;
  update_log?: string;
  package_name?: string;
  description?: string;
}
