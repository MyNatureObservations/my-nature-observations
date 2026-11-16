<script lang="ts">
 import {page} from "$app/state";
 import BirdListPage from "$lib/components/BirdListPage.svelte";
</script>
{#key `${page.params.id}:${page.params.listId}`}<BirdListPage entryId={page.params.id!} listId={page.params.listId!}/>{/key}
