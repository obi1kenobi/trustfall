# Querying RSS and Atom feeds

This demo project runs Trustfall queries over the feeds of PCGamer and Wired magazines.

It downloads the feed contents as XML data files, then parses those files and runs Trustfall queries on them.

- [Example: Find articles containing game reviews in PCGamer](#example-find-articles-containing-game-reviews-in-pcgamer)
- [Example: Extract all links and titles as a list from each feed](#example-extract-all-links-and-titles-as-a-list-from-each-feed)

## Example: Find articles containing game reviews in PCGamer

Query: ([link](example_queries/game_reviews.ron))
```graphql
{
    Feed {
        title {
            content @filter(op: "has_substring", value: ["$feed"])
        }

        entry_: entries {
            title_: title {
                content @filter(op: "regex", value: ["$title_pattern"])
                        @output
            }

            links {
                link: href @output
            }

            content {
                body @output
            }
        }
    }
}
```
with arguments `{ "feed": "PCGamer" }`.

To run it:
```
$ cargo run --example feeds query ./examples/feeds/example_queries/game_reviews.ron
< ... cargo output ... >

{
  "entry_body": "\n                            \n                            <article>\n                                <p>There is a Super Mario Bros film hitting cinemas this week, and the question on everyone&apos;s lips seems to be: will it ruin my childhood? I hope not! But childhood destroying or not (Chris Pratt <a href=\"https://www.hollywoodreporter.com/movies/movie-news/chris-pratt-super-mario-bros-movie-mario-voice-reaction-premiere-1235365581/\"><u>reckons</u></a> you&apos;re safe, but he would say that), the critical response is surprisingly varied, [...]",
  "entry_link": "https://www.pcgamer.com/super-mario-bros-movie-chris-pratt-doesnt-ruin-the-movie-but-its-still-not-that-great",
  "entry_title_content": "Super Mario Bros movie review round-up: 'Chris Pratt doesn't ruin the movie' but it's still not that great"
}
```

## Example: Extract all links and titles as a list from each feed

Query: ([link](example_queries/feed_links.ron))
```graphql
{
    Feed {
        id @output
        feed_type @output

        title_: title {
            content @output
            content_type @output
        }

        entries @fold {
            title {
                entry_title: content @output
            }
            links {
                entry_link: href @output
            }
        }
    }
}
```

To run it:
```
$ cargo run --example feeds query ./examples/feeds/example_queries/feed_links.ron
< ... cargo output ... >

{
  "entry_link": [
    "https://www.pcgamer.com/my-favorite-multiplayer-game-of-2023-so-far-is-basically-competitive-hitman",
    "https://www.pcgamer.com/youtuber-bypasses-chatgpts-ethical-constraints-to-make-it-generate-working-windows-95-keys",
    "https://www.pcgamer.com/there-will-be-a-chirper-for-sure-in-cities-skylines-2-says-colossal-order-ceo",
    [...]
    "https://www.pcgamer.com/what-diablo-4-classes-did-we-love-the-most-during-the-beta",
    "https://www.pcgamer.com/samsungs-chip-boffins-couldnt-help-but-tell-chatgpt-their-secrets",
    "https://www.pcgamer.com/acer-predator-x32-fp-review"
  ],
  "entry_title": [
    "My favorite multiplayer game of 2023 so far is basically competitive Hitman",
    "YouTuber bypasses ChatGPT's ethical constraints to make it generate working Windows 95 keys",
    "'There will be a Chirper, for sure' in Cities: Skylines 2, says Colossal Order CEO",
    [...]
    "Diablo 4 class impressions: Sorcerers are OP and Barbarians need more time in the gym",
    "Samsung's chip boffins couldn't help but tell ChatGPT their secrets",
    "Acer Predator X32 FP"
  ],
  "feed_type": "RSS2",
  "id": "6ea975addf5d7fbd1a01263fc9b99152",
  "title_content": "PCGamer latest",
  "title_content_type": "text/plain"
}

{
  "entry_link": [
    "https://www.wired.com/story/trump-mug-shot-privacy/",
    "https://www.wired.com/story/one-citys-escape-plan-from-rising-seas/",
    "https://www.wired.com/story/overwatch-2-support-hero-lifeweaver/",
    "https://www.wired.com/story/best-photo-printing-services/",
    "https://www.wired.com/story/platforms-design-ux-affordances/",
    [...]
    "https://www.wired.com/review/system76-pangolin-linux-laptop/",
    "https://www.wired.com/story/large-language-model-phishing-scams/",
    "https://www.wired.com/story/burning-man-climate-death-spiral/"
  ],
  "entry_title": [
    "A Mug Shot Could Play Right Into Trump’s Hands",
    "What Would Strategic Relocation from Charleston Look Like?",
    "The Latest ‘Overwatch 2’ Hero Is Going to Start a Class War",
    "8 Best Photo Printing Services (2023): Tips, Print Quality, and More",
    "There’s No Such Thing as a One-Size-Fits-All Web",
    [...]
    "System76 Pangolin Review: A 15-Inch Linux Laptop for the Masses",
    "Brace Yourself for a Tidal Wave of ChatGPT Email Scams",
    "Can Burning Man Pull Out of Its Climate Death Spiral?"
  ],
  "feed_type": "RSS2",
  "id": "6e6444830b6d094a37e5f2b0bac76c8",
  "title_content": "Feed: All Latest",
  "title_content_type": "text/plain"
}
```
