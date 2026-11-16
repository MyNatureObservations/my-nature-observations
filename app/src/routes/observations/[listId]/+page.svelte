<script lang="ts">
 import {page} from "$app/state";
 import BirdListPage from "$lib/components/BirdListPage.svelte";
</script>
{#key page.params.listId}<BirdListPage entryId={null} listId={page.params.listId!}/>{/key}
