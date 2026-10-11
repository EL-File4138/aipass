export function isExtensionVersion(version) {
  if (typeof version !== "string" || !/^(0|[1-9]\d*)(\.(0|[1-9]\d*)){0,3}$/.test(version)) return false;
  const parts = version.split(".").map(Number);
  return parts.every((part) => part <= 65535) && parts.some((part) => part > 0);
}
