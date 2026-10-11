import type { ToolSwitchRequest, ToolConfigPreview, ToolConfigApplyResult } from "../types";

export function toolConfigActions(invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>, clearError: () => void) {
  return {
    preview(request: ToolSwitchRequest) {
      clearError();
      return invoke<ToolConfigPreview>("tool_config_preview", { request });
    },
    apply(request: ToolSwitchRequest) {
      clearError();
      return invoke<ToolConfigApplyResult>("tool_config_apply", { request });
    }
  };
}
