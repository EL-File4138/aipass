import { invoke } from "@tauri-apps/api/core";

// Copy directly in Rust so API keys never traverse the WebView for this action.
export const copyCredential = (id: string, field: string) => invoke<boolean>("secret_copy", { id, field });
export const copySensitive = (value: string) => invoke<boolean>("clipboard_write_sensitive", { value });
export async function clearSensitiveClipboard(): Promise<void> {
  try { await invoke("clipboard_clear_sensitive"); }
  catch { /* Native timer retries cleanup if the clipboard is temporarily unavailable. */ }
}
