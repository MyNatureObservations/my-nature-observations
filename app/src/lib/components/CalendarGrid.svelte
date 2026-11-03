<script lang="ts">
  import { language } from "$lib/language.svelte";

  let {
    displayedMonth = $bindable(),
    selectedDate = null,
    markedDates = [],
    onSelectDate,
    onGoToToday,
    isMonthPickerOpen = $bindable(false),
  }: {
    displayedMonth: Date;
    selectedDate?: string | null;
    markedDates?: string[];
    onSelectDate?: (date: string) => void;
    onGoToToday?: () => void;
    isMonthPickerOpen?: boolean;
  } = $props();

  const locale = $derived(language.current === "sv" ? "sv-SE" : "en-GB");

  let pickerYear = $state(displayedMonth.getFullYear());
  let isEditingYear = $state(false);
  let draftYear = $state("");

  function changePickerYear(offset: number) {
    pickerYear += offset;
  }

  function startEditingYear() {
    draftYear = String(pickerYear);
    isEditingYear = true;
  }

  function savePickerYear() {
    const year = Number(draftYear);

    if (!Number.isInteger(year) || year < 1 || year > 9999) {
      return;
    }

    pickerYear = year;
    isEditingYear = false;
  }

  function cancelEditingYear() {
    isEditingYear = false;
  }

  const pickerMonths = $derived(
    Array.from({ length: 12 }, (_, index) => ({
      index,
      name: new Intl.DateTimeFormat(locale, {
        month: "short",
      }).format(new Date(2026, index, 1)),
    })),
  );

  function selectPickerMonth(month: number) {
    displayedMonth = new Date(pickerYear, month, 1);
    closeMonthPicker();
  }

  function goToToday() {
    const today = new Date();

    displayedMonth = new Date(today.getFullYear(), today.getMonth(), 1);

    closeMonthPicker();
    onGoToToday?.();
  }

  function openMonthPicker() {
    pickerYear = displayedMonth.getFullYear();
    isEditingYear = false;
    isMonthPickerOpen = true;
  }

  function closeMonthPicker() {
    isMonthPickerOpen = false;
  }

  const monthTitle = $derived(
    new Intl.DateTimeFormat(locale, {
      month: "long",
      year: "numeric",
    }).format(displayedMonth),
  );

  // Monday is always the first day of the week.
  const weekdays = $derived(
    Array.from({ length: 7 }, (_, index) => {
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
      ...Array<number | null>(offset).fill(null),
      ...Array.from({ length: daysInMonth }, (_, i) => i + 1),
    ];
  });

  function selectDay(day: number) {
    const year = displayedMonth.getFullYear();
    const month = String(displayedMonth.getMonth() + 1).padStart(2, "0");
    const date = String(day).padStart(2, "0");

    onSelectDate?.(`${year}-${month}-${date}`);
  }

  function dateKey(day: number): string {
    const year = displayedMonth.getFullYear();
    const month = String(displayedMonth.getMonth() + 1).padStart(2, "0");
    const date = String(day).padStart(2, "0");

    return `${year}-${month}-${date}`;
  }

  function hasContent(day: number): boolean {
    return markedDates.includes(dateKey(day));
  }

  function isToday(day: number): boolean {
    const today = new Date();

    return (
      displayedMonth.getFullYear() === today.getFullYear() &&
      displayedMonth.getMonth() === today.getMonth() &&
      day === today.getDate()
    );
  }

  function changeMonth(offset: number) {
    displayedMonth = new Date(
      displayedMonth.getFullYear(),
      displayedMonth.getMonth() + offset,
      1,
    );
  }
</script>

<div class="calendar-grid-component">
  {#if isMonthPickerOpen}
    <div class="month-picker">
      <div class="year-navigation">
        <button
          type="button"
          class="year-arrow"
          aria-label={language.current === "sv"
            ? "Föregående år"
            : "Previous year"}
          onclick={() => changePickerYear(-1)}
        >
          ‹
        </button>

        {#if isEditingYear}
          <input
            type="number"
            class="picker-year-input"
            min="1"
            max="9999"
            bind:value={draftYear}
            aria-label={language.current === "sv" ? "Ange år" : "Enter year"}
            onkeydown={(event) => {
              if (event.key === "Enter") savePickerYear();
              if (event.key === "Escape") cancelEditingYear();
            }}
          />

          <button
            type="button"
            class="picker-year-confirm"
            onclick={savePickerYear}
            aria-label={language.current === "sv"
              ? "Bekräfta år"
              : "Confirm year"}
          >
            ✓
          </button>
        {:else}
          <button
            type="button"
            class="picker-year"
            onclick={startEditingYear}
            aria-label={language.current === "sv" ? "Ändra år" : "Change year"}
          >
            {pickerYear}
          </button>
        {/if}
        <button
          type="button"
          class="year-arrow"
          aria-label={language.current === "sv" ? "Nästa år" : "Next year"}
          onclick={() => changePickerYear(1)}
        >
          ›
        </button>
      </div>
      <div class="month-picker-grid">
        {#each pickerMonths as month}
          <button
            type="button"
            class="month-picker-option"
            class:current-month={pickerYear === displayedMonth.getFullYear() &&
              month.index === displayedMonth.getMonth()}
            aria-pressed={pickerYear === displayedMonth.getFullYear() &&
              month.index === displayedMonth.getMonth()}
            onclick={() => selectPickerMonth(month.index)}
          >
            {month.name}
          </button>
        {/each}
      </div>
      <div class="month-picker-footer">
        <button type="button" class="month-picker-today" onclick={goToToday}>
          {language.current === "sv" ? "Idag" : "Today"}
        </button>
      </div>
    </div>
  {:else}
    <div class="calendar-header">
      <button
        type="button"
        class="month-arrow"
        aria-label={language.current === "sv"
          ? "Föregående månad"
          : "Previous month"}
        onclick={() => changeMonth(-1)}
      >
        ‹
      </button>

      <button
        type="button"
        class="month-title"
        aria-label={language.current === "sv"
          ? "Välj månad och år"
          : "Choose month and year"}
        aria-expanded={isMonthPickerOpen}
        onclick={openMonthPicker}
      >
        <span>{monthTitle}</span>
        <span class="month-title-chevron" aria-hidden="true">⌄</span>
      </button>

      <button
        type="button"
        class="month-arrow"
        aria-label={language.current === "sv" ? "Nästa månad" : "Next month"}
        onclick={() => changeMonth(1)}
      >
        ›
      </button>
    </div>
    <div class="calendar-grid">
      {#each weekdays as weekday}
        <span class="weekday" aria-label={weekday.full}>
          {weekday.short}
        </span>
      {/each}

      {#each calendarDays as day}
        {#if day === null}
          <span class="empty-day"></span>
        {:else}
          <button
            type="button"
            class="calendar-day"
            class:today={isToday(day)}
            class:selected={selectedDate === dateKey(day)}
            aria-pressed={selectedDate === dateKey(day)}
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
            onclick={() => selectDay(day)}
          >
            <span class="day-number">{day}</span>

            {#if hasContent(day)}
              <span class="day-content-dot" aria-hidden="true"></span>
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .calendar-grid-component {
    font-family: Georgia, "Times New Roman", serif;
    color: #343530;
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

  .month-arrow:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 2px;
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

  .calendar-day:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 2px;
  }

  .calendar-day.today {
    box-shadow: inset 0 0 0 1px #788e7b;
  }

  .calendar-day.selected {
    background: var(--accent, #435b47);
    color: #ffffff;
  }

  .calendar-day.today.selected {
    box-shadow: inset 0 0 0 1px #435b47;
  }

  .month-title {
    border: none;
    background: transparent;
    color: #343530;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }

  .month-title-chevron {
    color: #788e7b;
    font-size: 16px;
    line-height: 1;
  }

  .month-title:hover {
    color: var(--accent, #435b47);
  }

  .month-title:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 3px;
  }

  .month-picker {
    min-height: 220px;
  }

  .year-navigation {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 4px;
  }

  .picker-year {
    padding: 4px 8px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: #343530;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 15px;
    font-weight: 500;
    cursor: pointer;
  }

  .picker-year:hover {
    background: #eef1eb;
  }

  .picker-year-input {
    width: 76px;
    padding: 4px 6px;
    border: 1px solid #d9d8d1;
    border-radius: 5px;
    background: transparent;
    color: #343530;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 15px;
    text-align: center;
  }

  .picker-year-confirm {
    padding: 4px;
    border: none;
    background: transparent;
    color: #788e7b;
    cursor: pointer;
  }

  .picker-year:focus-visible,
  .picker-year-input:focus-visible,
  .picker-year-confirm:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 2px;
  }

  .year-arrow {
    border: none;
    background: transparent;
    color: #788e7b;
    font-size: 24px;
    cursor: pointer;
  }

  .year-arrow:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 2px;
  }

  .month-picker-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 4px 8px;
    margin-top: 16px;
  }

  .month-picker-option {
    min-height: 32px;
    padding: 5px 4px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: #343530;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 13px;
    cursor: pointer;
  }

  .month-picker-option:hover {
    background: #eef1eb;
  }

  .month-picker-option.current-month {
    background: var(--accent, #435b47);
    color: #ffffff;
  }

  .month-picker-option:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 2px;
  }

  .month-picker-footer {
    display: flex;
    justify-content: center;
    margin-top: 14px;
  }

  .month-picker-today {
    padding: 6px 12px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--accent, #435b47);
    font-family: Georgia, "Times New Roman", serif;
    font-size: 13px;
    cursor: pointer;
  }

  .month-picker-today:hover {
    background: #eef1eb;
  }

  .month-picker-today:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 2px;
  }

  .calendar-day {
    position: relative;
  }

  .day-content-dot {
    position: absolute;
    bottom: 3px;
    left: 50%;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: #788e7b;
    transform: translateX(-50%);
    pointer-events: none;
  }

  .calendar-day.selected .day-content-dot {
    background: #f8f7f3;
  }
</style>
