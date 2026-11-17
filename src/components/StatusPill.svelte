<script lang="ts">
  import { STATUS_LABEL, type Status } from "../lib/types";

  let { status }: { status: Status | null } = $props();
</script>

<span class="pill {status ?? ''}">{status ? STATUS_LABEL[status] : "Non badgé"}</span>
