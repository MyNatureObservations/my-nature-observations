<script lang="ts">
  import { tick } from "svelte";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  let { value = $bindable() }: { value: string } = $props();

  let isEditing = $state(false);
  let isCalendarOpen = $state(false);
  let draftDate = $state("");
  let errorMessage = $state("");
  let dateInput = $state<HTMLInputElement | undefined>(undefined);
  let datePickerElement: HTMLDivElement;

  let displayedMonth = $state(new Date(`${value}T12:00:00`));

  function openCalendar() {
    displayedMonth = new Date(`${value}T12:00:00`);
    isEditing = false;
    errorMessage = "";
    isCalendarOpen = true;
  }

  function closeCalendar() {
    isCalendarOpen = false;
  }

  function handleCalendarKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && isCalendarOpen) {
      closeCalendar();
    }
  }

  function handleWindowClick(event: MouseEvent) {
    if (
      isCalendarOpen &&
      datePickerElement &&
      !datePickerElement.contains(event.target as Node)
    ) {
      closeCalendar();
    }
  }

  const locale = $derived(language.current === "sv" ? "sv-SE" : "en-GB");

  const monthTitle = $derived(
    new Intl.DateTimeFormat(locale, {
      month: "long",
      year: "numeric",
    }).format(displayedMonth),
  );

  // Start the calendar on Monday, regardless of the selected language.
  const weekdays = $derived(
    Array.from({ length: 7 }, (_, index) => {
      // Use a known Monday to keep weekday labels in Monday-first order.
      const monday = new Date(2026, 9, 12);
      monday.setDate(monday.getDate() + index);

      return {
        short: new Intl.DateTimeFormat(locale, {
          weekday: "short",
        }).format(monday),
        full: new Intl.DateTimeFormat(locale, {
          weekday: "long",
        }).format(monday),
      };
    }),
  );

  const calendarDays = $derived.by(() => {
    const year = displayedMonth.getFullYear();
    const month = displayedMonth.getMonth();

    const firstDay = new Date(year, month, 1).getDay();
    const offset = (firstDay + 6) % 7;
    const daysInMonth = new Date(year, month + 1, 0).getDate();

    return [
      ...Array(offset).fill(null),
      ...Array.from({ length: daysInMonth }, (_, i) => i + 1),
    ];
  });

  function changeMonth(offset: number) {
    displayedMonth = new Date(
      displayedMonth.getFullYear(),
      displayedMonth.getMonth() + offset,
      1,
    );
  }

  function selectDay(day: number) {
    const year = displayedMonth.getFullYear();
    const month = String(displayedMonth.getMonth() + 1).padStart(2, "0");
    const date = String(day).padStart(2, "0");

    value = `${year}-${month}-${date}`;
    closeCalendar();
  }

  const formattedDate = $derived(
    new Intl.DateTimeFormat(language.current === "sv" ? "sv-SE" : "en-GB", {
      day: "numeric",
      month: "long",
      year: "numeric",
    }).format(new Date(`${value}T12:00:00`)),
  );

  function isValidDate(date: string): boolean {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) {
      return false;
    }

    const [year, month, day] = date.split("-").map(Number);
    const parsed = new Date(year, month - 1, day);

    return (
      parsed.getFullYear() === year &&
      parsed.getMonth() === month - 1 &&
      parsed.getDate() === day
    );
  }

  async function startEditing() {
    isCalendarOpen = false;
    draftDate = value;
    errorMessage = "";
    isEditing = true;

    await tick();
    dateInput?.focus();
    dateInput?.select();
  }

  function saveDate() {
    if (!isValidDate(draftDate)) {
      errorMessage = m.date_invalid({}, { locale: language.current });
      return;
    }

    value = draftDate;
    errorMessage = "";
    isEditing = false;
  }

  function cancelEditing() {
    errorMessage = "";
    isEditing = false;
  }

  function handleDateBlur() {
    if (isValidDate(draftDate)) {
      saveDate();
    } else {
      errorMessage = m.date_invalid({}, { locale: language.current });
    }
  }
</script>

<svelte:window onkeydown={handleCalendarKeydown} onclick={handleWindowClick} />

<div class="date-picker" bind:this={datePickerElement}>
  {#if isEditing}
    <div class="date-edit">
      <input
        type="text"
        bind:this={dateInput}
        bind:value={draftDate}
        aria-label={m.date_entry_label({}, { locale: language.current })}
        aria-invalid={errorMessage !== ""}
        onblur={handleDateBlur}
        onkeydown={(event) => {
          if (event.key === "Enter") saveDate();
          if (event.key === "Escape") cancelEditing();
        }}
      />
    </div>

    {#if errorMessage}
      <p class="error" role="alert">{errorMessage}</p>
    {/if}
  {:else}
    <div class="date-controls">
      <button
        type="button"
        class="date-display"
        onclick={startEditing}
        aria-label={m.date_edit_label({}, { locale: language.current })}
      >
        {formattedDate}
      </button>

      <button
        type="button"
        class="calendar-trigger"
        onclick={openCalendar}
        aria-label={m.date_open_calendar({}, { locale: language.current })}
        aria-expanded={isCalendarOpen}
      >
        <svg
          width="15"
          height="15"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <rect x="3" y="5" width="18" height="16" rx="2" />
          <path d="M7 3v4M17 3v4M3 10h18" />
        </svg>
      </button>
    </div>

    {#if isCalendarOpen}
      <div class="calendar-panel">
        <div class="calendar-header">
          <button
            type="button"
            class="month-arrow"
            onclick={() => changeMonth(-1)}
            aria-label={m.date_previous_month({}, { locale: language.current })}
            >‹</button
          >

          <span class="month-title">
            {monthTitle}
          </span>

          <button
            type="button"
            class="month-arrow"
            onclick={() => changeMonth(1)}
            aria-label={m.date_next_month({}, { locale: language.current })}
            >›</button
          >
        </div>

        <div class="calendar-grid">
          {#each weekdays as weekday}
            <span class="weekday" aria-label={weekday.full}>
              {weekday.short}
            </span>
          {/each}

          {#each calendarDays as day, index}
            {#if day === null}
              <span class="empty-day"></span>
            {:else}
              <button
                type="button"
                class="calendar-day"
                class:selected={value ===
                  `${displayedMonth.getFullYear()}-${String(displayedMonth.getMonth() + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`}
                aria-label={new Intl.DateTimeFormat(locale, {
                  day: "numeric",
                  month: "long",
                  year: "numeric",
                }).format(
                  new Date(
                    displayedMonth.getFullYear(),
                    displayedMonth.getMonth(),
                    day,
                  ),
                )}
                aria-pressed={value ===
                  `${displayedMonth.getFullYear()}-${String(displayedMonth.getMonth() + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`}
                onclick={() => selectDay(day)}
              >
                {day}
              </button>
            {/if}
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .date-picker {
    position: relative;
    margin-bottom: 20px;
    font-family: Georgia, "Times New Roman", serif;
  }

  .date-display {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-muted, #686d63);
    font-family: Georgia, "Times New Roman", serif;
    font-size: 13px;
    letter-spacing: 0.3px;
    cursor: pointer;
  }

  .date-display:hover {
    color: #435b47;
  }

  .date-display:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 5px;
    border-radius: 2px;
  }

  .date-edit {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .date-edit input {
    width: 130px;
    padding: 7px 10px;
    border: 1px solid transparent;
    border-bottom-color: #d9d8d1;
    border-radius: 0;
    background: transparent;
    color: #343530;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 13px;
    letter-spacing: 0.3px;
    transition: border-color 0.2s ease;
  }

  .date-edit input:focus {
    outline: none;
    border-bottom-color: #788e7b;
  }

  .date-edit input::selection {
    background: #cbd8c9;
    color: #343530;
  }

  .error {
    margin-top: 8px;
    color: #a33b35;
    font-size: 12px;
  }

  .calendar-panel {
    position: absolute;
    top: calc(100% + 12px);
    left: 0;
    z-index: 20;
    width: 290px;
    padding: 18px;
    background: #f8f7f3;
    border: 1px solid #d9d8d1;
    border-radius: 10px;
    box-shadow: 0 12px 30px rgba(52, 53, 48, 0.1);
  }

  .calendar-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 18px;
  }

  .month-title {
    color: #343530;
    font-size: 13px;
    font-weight: 500;
  }

  .month-arrow {
    border: none;
    background: transparent;
    color: #788e7b;
    font-size: 24px;
    cursor: pointer;
  }

  .calendar-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 4px;
  }

  .weekday {
    text-align: center;
    font-size: 11px;
    color: var(--text-muted, #686d63);
    padding: 6px 0;
  }

  .empty-day {
    min-height: 30px;
  }

  .calendar-day {
    height: 30px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: #343530;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 12px;
    cursor: pointer;
  }

  .calendar-day:hover {
    background: #eef1eb;
  }

  .calendar-day.selected {
    background: var(--accent, #435b47);
    color: #ffffff;
  }

  .calendar-day:focus-visible,
  .month-arrow:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 2px;
  }

  .date-controls {
    display: inline-flex;
    align-items: center;
    gap: 10px;
  }

  .calendar-trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    border: none;
    background: transparent;
    color: #788e7b;
    cursor: pointer;
  }

  .calendar-trigger:hover {
    color: #435b47;
  }

  .calendar-trigger:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 4px;
    border-radius: 3px;
  }
</style>
