# These settings are synced to GitHub by https://probot.github.io/apps/settings/

repository:
  description: A query engine for any combination of data sources. Query your files and APIs as if they were databases!
  topics: rust python wasm javascript query-language
  has_issues: true
  has_projects: true
  has_wiki: true
  has_downloads: true
  default_branch: main

  allow_squash_merge: true
  allow_merge_commit: false
  allow_rebase_merge: false

  allow_auto_merge: true
  delete_branch_on_merge: true

labels:
  - name: A-syntax
    color: '#f7e101'
    description: "Area: query or schema syntax"
  - name: A-frontend
    color: '#f7e101'
    description: "Area: turning queries into IR"
  - name: A-ir
    color: '#f7e101'
    description: "Area: compiler intermediate representation"
  - name: A-interpreter
    color: '#f7e101'
    description: "Area: executing IR using the interpreter"
  - name: A-adapter
    color: '#f7e101'
    description: "Area: plugging data sources into the interpreter"
  - name: A-schema
    color: '#f7e101'
    description: "Area: the data model that queries use"
  - name: A-errors
    color: '#f7e101'
    description: "Area: external-facing error functionality"
  - name: A-docs
    color: '#f7e101'
    description: "Area: documentation for the query language or library APIs"
    oldname: documentation
  - name: C-bug
    color: '#b60205'
    description: "Category: doesn't meet expectations"
    oldname: bug
  - name: C-enhancement
    color: '#1d76db'
    description: "Category: raise the bar on expectations"
    oldname: enhancement
  - name: C-maintainability
    color: '#1d76db'
    description: "Category: reduce maintenance burden, e.g. by clearing tech debt or improving test infra"
  - name: C-feature-request
    color: '#1d76db'
    description: "Category: request for new future functionality"
  - name: E-help-wanted
    color: '#02e10c'
    description: "Call for participation: help is requested to fix this issue."
  - name: E-mentor
    color: '#02e10c'
    description: "Call for participation: mentorship is available for this issue."
  - name: E-easy
    color: '#02e10c'
    description: "Call for participation: experience needed to fix: easy / not much (good first issue)"
  - name: E-medium
    color: '#02e10c'
    description: "Call for participation: experience needed to fix: medium / intermediate"
  - name: E-hard
    color: '#02e10c'
    description: "Call for participation: experience needed to fix: hard / a lot."
  - name: L-rust
    color: "#fb4c8e"
    description: "Language: affects use cases in the Rust programming language"
  - name: L-python
    color: "#fb4c8e"
    description: "Language: affects use cases in the Python programming language"
  - name: L-nodejs
    color: "#fb4c8e"
    description: "Language: affects use cases in Node.js"
  - name: L-browser
    color: "#fb4c8e"
    description: "Language: affects use cases in web browsers"
  - name: S-waiting-on-author
    color: "#f027a1"
    description: "Status: awaiting some action (such as code changes or more information) from the author."
  - name: S-blocked
    color: "#f027a1"
    description: "Status: marked as blocked ❌ on something else such as an RFC or other implementation work."
  - name: S-experimental
    color: "#f027a1"
    description: ""
  - name: S-needs-review
    color: "#f027a1"
    description: "Status: awaiting review from maintainers but also interested parties."
  - name: S-needs-funding
    color: "#f027a1"
    description: "Status: this feature has a maintainability risk or other risks, and is blocked on having a source of funding in order to not be disruptive to the project."
  - name: R-breaking-change
    color: "#2e00c0"
    description: "Release: implementing or merging this will introduce a breaking change."
  - name: R-relnotes
    color: "#2e00c0"
    description: "Release: document this in the release notes of the next release"

branches:
  - name: main
    # https://docs.github.com/en/rest/reference/repos#update-branch-protection
    # Branch Protection settings. Set to null to disable.
    protection:
      # Required. Require at least one approving review on a pull request, before merging. Set to null to disable.
      required_pull_request_reviews:
        require_code_owner_reviews: true
      # Required. Require that conversations are resolved before merging.
      # We disable this.
      required_conversation_resolution: null
      # Required. Require status checks to pass before merging. Set to null to disable
      required_status_checks:
        # Required. Require branches to be up to date before merging.
        # We disable this.
        strict: null
        # Required. The list of status checks to require in order to merge into this branch
        contexts: [
          "All CI stages"
        ]
      # Required. Enforce all configured restrictions for administrators. Set to true to enforce required status checks for repository administrators. Set to null to disable.
      # We disable this.
      enforce_admins: false
      # Required. Restrict who can push to this branch. Team and user restrictions are only available for organization-owned repositories. Set to null to disable.
      restrictions: null
      # Prevent merge commits from being pushed to matching branches
      required_linear_history: true
