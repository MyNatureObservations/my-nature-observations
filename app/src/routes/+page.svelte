<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import { language } from "$lib/language.svelte";
</script>

<svelte:head>
  <title>My Nature Observations</title>
  <meta
    name="description"
    content={m.home_meta_description({}, { locale: language.current })}
  />
</svelte:head>

<main class="home">
  <div class="home-content">
    <section class="journal-preview">
      <span class="section-label">
        {m.journal_section({}, { locale: language.current })}
      </span>
      <div class="journal-cards">
        <a href="/journal/new" class="journal-card">
          <span class="card-label">
            {m.home_card_new_label({}, { locale: language.current })}
          </span>

          <h2>{m.home_card_new_title({}, { locale: language.current })}</h2>

          <p>
            {m.home_card_new_description({}, { locale: language.current })}
          </p>

          <span class="card-arrow" aria-hidden="true">→</span>
        </a>

        <a href="/journal?latest=true" class="journal-card">
          <span class="card-label">
            {m.home_card_continue_label({}, { locale: language.current })}
          </span>

          <h2>
            {m.home_card_continue_title({}, { locale: language.current })}
          </h2>

          <p>
            {m.home_card_continue_description({}, { locale: language.current })}
          </p>

          <span class="card-arrow" aria-hidden="true">→</span>
        </a>
      </div>
    </section>
    <div class="welcome-message">
      <div class="welcome-intro">
        <h1>{m.home_heading({}, { locale: language.current })}</h1>
      </div>
    </div>
  </div>

  <footer>{m.home_footer({}, { locale: language.current })}</footer>
</main>

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(html) {
    background: #f8f7f3;
  }

  :global(body) {
    margin: 0;
    color: #343530;
    background: #f8f7f3;
    font-family: Georgia, "Times New Roman", serif;
    -webkit-font-smoothing: antialiased;
  }

  .home {
    min-height: calc(100vh - 40px);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 64px 64px 24px;
  }

  .home-content {
    width: 100%;
    max-width: 1100px;
    margin: 0 auto;
  }

  .section-label {
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 2.5px;
    color: var(--accent, #435b47);
  }

  h1 {
    margin: 0 0 24px;
    font-size: clamp(36px, 5vw, 52px);
    font-weight: 400;
    line-height: 1.2;
    letter-spacing: -1px;
  }

  .journal-cards {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 20px;
    margin-top: 24px;
  }

  .journal-card {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 200px;
    padding: 28px 32px;
    border: 1px solid #e4e3dd;
    border-radius: 8px;
    background: #fbfaf7;
    color: inherit;
    text-decoration: none;
    transition:
      border-color 0.2s ease,
      background-color 0.2s ease;
  }

  .journal-card:hover {
    border-color: #bfc8bd;
    background: #fdfcf9;
  }

  .journal-card:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 3px;
  }

  .card-label {
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 2px;
    color: var(--accent, #435b47);
  }

  .journal-card h2 {
    margin: 22px 0 10px;
    font-size: 27px;
    font-weight: 400;
    line-height: 1.3;
  }

  .journal-card p {
    margin: 0;
    font-size: 16px;
    line-height: 1.5;
    color: var(--text-muted, #686d63);
  }

  .card-arrow {
    align-self: flex-end;
    margin-top: auto;
    padding-top: 16px;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 20px;
    color: #788e7b;
  }

  .welcome-message {
    margin-top: 64px;
    text-align: center;
  }

  .welcome-message h1 {
    margin: 0 auto 20px;
    max-width: 750px;
    font-size: 32px;
    line-height: 1.3;
  }

  footer {
    margin-top: 100px;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 11px;
    letter-spacing: 0.5px;
    color: var(--text-muted, #686d63);
    text-align: center;
  }

  @media (max-width: 600px) {
    .home {
      padding: 56px 28px 24px;
    }

    h1 {
      margin-top: 36px;
    }

    .journal-cards {
      grid-template-columns: 1fr;
    }

    .journal-card {
      min-height: 170px;
    }
  }

  .welcome-message {
    position: relative;
    min-height: 150px;
  }

  .welcome-intro {
    animation: welcome-fade-out 2s ease-in-out 1.5s both;
  }

  @keyframes welcome-fade-out {
    from {
      opacity: 1;
    }

    to {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .welcome-intro {
      animation: none;
      opacity: 0;
    }
  }
</style>
