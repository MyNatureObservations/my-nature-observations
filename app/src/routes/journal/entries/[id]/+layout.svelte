<script lang="ts">
  import { page } from "$app/state";

  let { children } = $props();
</script>

<!-- Reset date loading and the editor together, after navigation is allowed. -->
{#key page.params.id}
  {@render children()}
{/key}
