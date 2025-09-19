# Trustfall documentation

## Development

To install dependencies first install [uv](https://github.com/astral-sh/uv) if you
haven't already and then:

```
uv sync
```

Then, inside the project you can run the commands as::

* `uv run mkdocs new [dir-name]` - Create a new project.
* `uv run mkdocs serve` - Start the live-reloading docs server.
* `uv run mkdocs build` - Build the documentation site.
* `uv run mkdocs -h` - Print help message and exit.

## Project layout

    mkdocs.yml    # The configuration file.
    docs/
        index.md  # The documentation homepage.
        ...       # Other markdown pages, images and other files.
