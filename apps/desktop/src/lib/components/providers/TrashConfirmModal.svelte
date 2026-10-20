<script lang="ts">
  import type { ProviderEntry } from "@aipass/schemas";
  import { t } from "../../stores/i18n";
  import type { MaybePromise } from "../../types";
  import ConfirmModal from "../shared/ConfirmModal.svelte";

  export let onDelete: (id: string) => MaybePromise = () => {};
  export let onEmpty: () => MaybePromise = () => {};
  export let onError: (message: string) => void = () => {};

  let open = false;
  let target: Pick<ProviderEntry, "id" | "title"> | undefined;

  export function requestDelete(entry: ProviderEntry | undefined) {
    if (!entry || open) return;
    target = { id: entry.id, title: entry.title };
    open = true;
  }

  export function requestEmpty() {
    if (open) return;
    target = undefined;
    open = true;
  }

  async function confirm() {
    try {
      if (target) await onDelete(target.id);
      else await onEmpty();
      return true;
    } catch (error) {
      onError(String(error));
      return false;
    }
  }
</script>

<ConfirmModal
  bind:open
  title={target ? $t("confirm.deleteProvider", { title: target.title }) : $t("confirm.emptyTrash")}
  description={$t("confirm.cannotUndo")}
  confirmLabel={$t(target ? "providerDetail.deleteForever" : "providerList.emptyTrash")}
  cancelLabel={$t("common.cancel")}
  onConfirm={confirm}
/>
