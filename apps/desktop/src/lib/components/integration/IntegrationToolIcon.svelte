<script lang="ts">
  import { Terminal } from "lucide-svelte";
  import type { ToolConfigTarget } from "../../types";

  export let tool: ToolConfigTarget;
  export let size = 17;

  const icons: Partial<Record<ToolConfigTarget, string>> = {
    codex: "/tool-icons/codex.svg",
    "claude-code": "/tool-icons/claude-code.svg",
    "gemini-cli": "/tool-icons/gemini-cli.svg",
    opencode: "/tool-icons/opencode.svg",
    grok: "/tool-icons/grok.svg",
    pi: "/tool-icons/pi.svg",
    cursor: "/tool-icons/cursor.svg",
    "kimi-code": "/tool-icons/kimi-code.svg",
    zed: "/tool-icons/zed.svg",
    "deepseek-harness": "/tool-icons/deepseek-harness.svg",
    "antigravity-cli": "/tool-icons/antigravity-cli.svg",
    "command-code": "/tool-icons/command-code.svg", workbuddy: "/tool-icons/workbuddy.svg",
    cline: "/tool-icons/cline.svg", trae: "/tool-icons/trae.svg", qoder: "/tool-icons/qoder.svg"
  };

  $: icon = icons[tool];
</script>

{#if icon}
<span
  class="brand-icon"
  style={`--brand-icon: url("${icon}"); --brand-icon-size: ${size}px`}
  aria-hidden="true"
></span>
{:else}<Terminal {size} aria-hidden="true" />{/if}

<style>
  .brand-icon {
    display: block;
    width: var(--brand-icon-size);
    height: var(--brand-icon-size);
    background: currentColor;
    -webkit-mask: var(--brand-icon) center / contain no-repeat;
    mask: var(--brand-icon) center / contain no-repeat;
  }
</style>
