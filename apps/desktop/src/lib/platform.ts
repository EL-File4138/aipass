declare const __AIPASS_TARGET_PLATFORM__: string;

// Build for the Tauri target, including cross builds. Rust remains responsible
// for rejecting unsupported CloudKit operations at the trusted boundary.
export const targetPlatform = typeof __AIPASS_TARGET_PLATFORM__ === "string"
  ? __AIPASS_TARGET_PLATFORM__ : "unknown";

export const supportsCloudKit = ["darwin", "macos"].includes(targetPlatform);
