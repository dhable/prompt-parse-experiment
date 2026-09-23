# prompt-parse-experiment

A small Rust spike that treats a prompt as a *language* rather than a blob of text.

It runs a [PEG grammar](https://docs.rs/peg) over a prompt and splits it into three
kinds of element — literal prose, tool references, and skill references — then prints
each bucket, prints the prompt with every reference rewritten to its wire form, and
prints a colorized inline diff of the before and after.

```
@`mezmo_internal:describe`   →  ToolRef { ns: "mezmo_internal", name: "describe" }
@`k8s:*`                     →  ToolRef { ns: "k8s", name: "*" }
@(skills/audit-report.md)    →  SkillRef { name: "skills/audit-report.md" }
```

## Hypothesis

**1. A sigil can mark references inside natural prose without firing on prose that
merely resembles a reference.**

This is the claim the demo prompt is built to attack. `@` on its own means nothing;
it is only significant when immediately followed by a delimiter — `` ` `` for a tool,
`(` for a skill (`tool_ref_start()` and `skill_ref_start()`, `src/main.rs:72,80`).
That requirement is what lets `ops@mezmo.com` sail through as literal content.
Backticks alone are not a reference either, so `` `not_a_tool:just_documentation` ``
stays prose. And fenced blocks are swallowed whole by `code_block_fence()`
(`src/main.rs:86`) before reference scanning can ever reach inside them, so
` ```not_a_tool:just_a_block``` ` is safe too. A prompt should be able to *talk about*
tools, paths, and email addresses without accidentally *invoking* them.

**2. Explicit references make a prompt statically analyzable.**

Once a reference is a node in a syntax tree instead of a regex guess, a harness can
do work on it before the model ever sees the text: resolve the namespace, catch the
typo'd tool name, notice that the skill file does not exist, check that the session
actually holds permission for what the prompt is about to reach for. Prompts today
fail at runtime, deep inside a turn. References that parse can fail at load time.

**3. A reference can bind to a set, not just to one tool.**

`` @`k8s:*` `` is a single reference that names a family. The namespace and the name
each accept the same glob vocabulary — wildcards, character classes, alternation
(`src/main.rs:56-64`) — that permission rules already use. If prompt authoring and
permission configuration share one pattern language, "which tools may this prompt
use" and "which tools does this prompt talk about" become the same question, asked
in the same syntax.

**What is not here.** Claims 2 and 3 are the motivation, not the implementation.
Nothing in this repo resolves a reference, validates it, or checks a permission. The
binary parses and rewrites; that is the whole of it.

## Syntax

| Source form | Parses to | Rewrites to |
|---|---|---|
| `` @`mezmo_internal:describe` `` | `ToolRef { ns, name }` | `` `mezmo_internal__describe` `` |
| `` @`k8s:*` `` | `ToolRef` with a glob name | `` `k8s__*` `` |
| `@(skills/audit-report.md)` | `SkillRef { name }` | `skills/audit-report.md` |

A tool reference is `ns:name` inside backticks. Both halves are *elements*, and an
element may be a plain literal or a glob:

| Pattern | Meaning |
|---|---|
| `*` / `?` | wildcard |
| `[abc]` | character class |
| `[!abc]` | negated character class |
| `{a,b}` | alternation, each branch itself globbable |

So `` @`k8s:get_{pod,deployment}` `` and `` @`mezmo_*:describe` `` are both
well-formed. Literal elements are capped at 64 characters (`literal_element()`,
`src/main.rs:63`), matching the tool-name length limit the major model providers
enforce.

A skill reference is a path inside parentheses: `@(skills/audit-report.md)`.

## Prior art: how harnesses spend their sigils

Every coding agent has landed on some subset of `@`, `/`, `#`, and `!`. None of
them agree on what those characters mean.

| Harness | `@` | `/` | Other |
|---|---|---|---|
| [Claude Code](https://code.claude.com/docs/en/interactive-mode) | file path mention — an autocomplete trigger | commands and skills, only at start of message | `!` shell mode; `:` emoji shortcode |
| [GitHub Copilot Chat](https://docs.github.com/en/copilot/reference/chat-cheat-sheet) | chat *participants* — `@workspace`, `@terminal`, `@vscode`, `@github` | commands | `#` context variables — `#file`, `#selection`, `#codebase`, `#git` |
| [Cursor](https://docs.cursor.com/en/context/@-symbols/@-web) | everything, via a taxonomy — `@Files`, `@Folders`, `@Code`, `@Docs`, `@Web`, `@Git`, `@Cursor Rules` | commands | — |
| [Cline](https://docs.cline.bot/features/at-mentions/overview) | paths *and* named providers, mixed — `@/path/to/file`, `@https://…`, `@problems`, `@terminal`, `@git-changes`, `@[commit-hash]` | commands | — |
| [Windsurf Cascade](https://docs.windsurf.com/windsurf/cascade) | files, directories, functions, classes, docs, web, MCP tools, past conversations | workflows (`/workflow-name`) | — |
| [Gemini CLI](https://google-gemini.github.io/gemini-cli/docs/cli/commands.html) | path injection — `@src/index.ts`, and **`@{path/to/file}`** in custom commands | commands | `!` shell passthrough, or bare `!` to toggle shell mode |
| [Codex CLI](https://developers.openai.com/codex/cli/slash-commands) | fuzzy file search that **inserts a bare path** | commands | `!` shell |
| [Aider](https://aider.chat/docs/usage/commands.html) | — none | `/add`, `/drop`, `/read-only` manage files out of band | `!` as alias for `/run` |

**`@` has converged on "bring something into context" and on nothing narrower.**
It names *files* in Claude Code, Codex CLI, and Gemini CLI; *agents* in Copilot;
and in Cursor, Windsurf, and [Continue.dev](https://docs.continue.dev/customize/deep-dives/custom-providers)
it names everything, at the cost of a
taxonomy (`@Files` vs `@Folders` vs `@Code` vs `@Docs`) you have to memorize.
Cline is the messiest and most honest about it, letting `@` carry a filesystem
path, a URL, and a fixed vocabulary of named providers in the same position, so
`@/src/api.ts` and `@problems` are the same syntactic slot holding categorically
different things. Copilot is the one harness that ran out of room: it needs `#`
for context variables precisely because it already spent `@` on participants. The
`@` in this experiment is yet another reading — a *tool or skill*, neither a file
nor an agent.

**These sigils are chosen as UI affordances, not as grammar.** Claude Code's own
docs describe `@` as a trigger for "file path autocomplete" — the character's job
is to open a picker, not to denote anything. Codex CLI takes this to its logical
end: `@` fuzzy-searches your workspace and then *inserts a bare path*, so the
sigil never survives into the prompt text at all. There is nothing left to parse
afterward, because the reference was resolved at typing time by a human pressing
Tab. That is the load-bearing assumption behind nearly every row of this table,
and it is the assumption this experiment drops. Prompts here arrive as plain text
from a file, a config, or an API, with no picker having sanitized anything and no
human available to disambiguate — which is the entire reason the delimiters are
mandatory rather than decorative.

**Position and mode do more disambiguating work than the characters do.** Claude
Code honors `/` and `!` only at the start of a message, which is what keeps them
from colliding with file paths and shell history. Gemini CLI's bare `!` toggles a
persistent shell *mode*. [Zed](https://zed.dev/blog/zed-ai)'s `/file` sits on its own line in an editable
buffer. Aider goes furthest and refuses inline references entirely: files enter
the chat through `/add` and `/drop`, out of band, so the prompt text stays pure
prose with no embedded syntax to misparse. Each of these solves ambiguity
structurally, by constraining *where* a sigil counts. This grammar takes the
opposite road — references are legal mid-sentence, anywhere prose can go — so all
of the disambiguation has to be carried by the delimiter pair instead.

**Where anyone does need a mid-prose reference, they reach for delimiters too.**
Gemini CLI's custom commands use `@{path/to/file}` rather than the bare `@path`
of its interactive mode, because once a reference is embedded in a stored
template there is no picker and no terminator — prose has to be able to resume
unambiguously after the path ends. That is the same problem and the same answer
as this experiment's `` @`ns:name` `` and `@(skills/path.md)`. The braces are not
decoration; they are the part that makes the text parseable without a human.

**The rewrite target is where the ecosystem actually constrains the design.**
References lower to `ns__name`, echoing MCP's `mcp__<server>__<tool>` convention.
That double underscore is not aesthetic: tool names reach model providers
unchanged, and both Anthropic's and OpenAI's tool-name patterns
(`^[a-zA-Z0-9_-]{1,64}$`) reject dots and colons. Separating source syntax from
wire format lets the authored form stay friendly — `ns:name` — while
transpilation absorbs the provider's constraints. The separator is not free of
collisions either: `:` is exactly what Claude Code's composer uses for emoji
shortcodes, which is a reminder that every character is already spent somewhere.
And Claude Code's [permission rules](https://code.claude.com/docs/en/permissions)
reportedly do *not* honor wildcards for MCP tools, which makes claim 3 above a
live question rather than a settled one.

## Running it

```sh
cargo run
```

Colors are dropped automatically when stdout is not a terminal; set `CLICOLOR_FORCE=1`
to force them through a pipe.

The demo prompt is hardcoded in `main()` (`src/main.rs:150`). Abridged output:

```
PromptElement::ToolRef
----------------------
[
    "ns=mezmo_internal, name=describe",
    "ns=k8s, name=*",
    "ns=mezmo_internal, name=describe",
]

PromptElement::SkillRef
[
    "skills/audit-report.md",
]

Prompt Diff
-----------
--- original
+++ transpiled
@@ -11,5 +11,5 @@

-    ... you should be able to use the @`mezmo_internal:describe` tool in order to
-    find out service specific details. Always include a kubernetes namespace on all @`k8s:*` tools.
+    ... you should be able to use the `mezmo_internal__describe` tool in order to
+    find out service specific details. Always include a kubernetes namespace on all `k8s__*` tools.
```

Note what the diff does *not* touch: `ops@mezmo.com`, the backticked
`` `find_service` `` and `` `not_a_tool:just_documentation` ``, and the fenced block
all pass through byte-for-byte.

## License

MIT. See [LICENSE](LICENSE).
