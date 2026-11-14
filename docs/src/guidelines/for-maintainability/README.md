# For Maintainability

*Is the code well-shaped and understandable?*

This is the index of coding guidelines from the **project maintainer**'s perspective.
Each guideline is anchored by a stable `short-name`
and listed with a one-line summary,
so a reader (or a review tool) can grasp the gist before opening the full text.

## Index

**[Design](design.md)**

- [`single-responsibility`](design.md#single-responsibility): Give each unit a single responsibility
- [`information-hiding`](design.md#information-hiding): Hide details behind interfaces
- [`open-closed`](design.md#open-closed): Be open for extension, closed for modification
- [`coupling-cohesion`](design.md#coupling-cohesion): Aim for loose coupling and strong cohesion
- [`consistency`](design.md#consistency): Be consistent

**[Naming](naming.md)**

- [`descriptive-names`](naming.md#descriptive-names): Be descriptive
- [`accurate-names`](naming.md#accurate-names): Be accurate
- [`bool-names`](naming.md#bool-names): Use assertion-style boolean names

**[Layout](layout.md)**

- [`top-down-reading`](layout.md#top-down-reading): Organize code for top-down reading
- [`logical-paragraphs`](layout.md#logical-paragraphs): Group statements into logical paragraphs
- [`block-expressions`](layout.md#block-expressions): Use block expressions to scope temporary state
- [`minimize-nesting`](layout.md#minimize-nesting): Minimize nesting
- [`explain-variables`](layout.md#explain-variables): Introduce explaining variables

**[Comments](comments.md)**

- [`explain-why`](comments.md#explain-why): Explain why, not what
- [`design-decisions`](comments.md#design-decisions): Document design decisions
- [`comment-punctuation`](comments.md#comment-punctuation): End sentence comments with punctuation

**[API Design and Visibility](api-design.md)**

- [`rust-type-invariants`](api-design.md#rust-type-invariants): Use types to enforce invariants
- [`enum-over-dyn`](api-design.md#enum-over-dyn): Prefer enum over trait objects for closed sets
- [`module-docs`](api-design.md#module-docs): Add module-level documentation for major components
- [`narrow-visibility`](api-design.md#narrow-visibility): Default to the narrowest visibility
- [`encode-intent-in-vis`](api-design.md#encode-intent-in-vis): Visibility modifiers encode intent, not just effect
- [`qualified-fn-imports`](api-design.md#qualified-fn-imports): Qualify function calls with the parent module
- [`workspace-deps`](api-design.md#workspace-deps): Use workspace dependencies
- [`no-bool-args`](api-design.md#no-bool-args): Avoid boolean arguments

**[Rust Conventions](rust-conventions.md)**

- [`kebab-case-crate-names`](rust-conventions.md#kebab-case-crate-names): Create crates with kebab-case names
- [`camel-case-acronyms`](rust-conventions.md#camel-case-acronyms): Follow Rust CamelCase and acronym capitalization
- [`closure-fn-suffix`](rust-conventions.md#closure-fn-suffix): End closure variables with `_fn`
- [`expect-dead-code`](rust-conventions.md#expect-dead-code): Use `#[expect(dead_code)]` with restraint
- [`alphabetical-attrs`](rust-conventions.md#alphabetical-attrs): Sort attributes and derive traits alphabetically
- [`narrow-lint-suppression`](rust-conventions.md#narrow-lint-suppression): Suppress lints at the narrowest scope
- [`rfc1574-summary`](rust-conventions.md#rfc1574-summary): Follow RFC 1574 summary line conventions
- [`no-impl-in-docs`](rust-conventions.md#no-impl-in-docs): Do not disclose implementation details in doc comments

**[Commits](commits.md)**

- [`refactor-then-feature`](commits.md#refactor-then-feature): Separate refactoring from features
