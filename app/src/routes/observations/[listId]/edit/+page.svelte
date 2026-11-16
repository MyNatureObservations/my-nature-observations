<script lang="ts">
 import {page} from "$app/state";
 import BirdListForm from "$lib/components/BirdListForm.svelte";
</script>
{#key page.params.listId}<BirdListForm editId={page.params.listId!}/>{/key}
