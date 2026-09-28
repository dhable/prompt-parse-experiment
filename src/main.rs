use clap::Parser;
use console::Style;
use similar::{ChangeTag, InlineChangeMode, InlineChangeOptions, TextDiff};
use std::fmt::{Display, Write};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Debug, PartialEq)]
struct ToolNamespace(Arc<str>);
impl Display for ToolNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, PartialEq)]
struct ToolName(Arc<str>);
impl Display for ToolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, PartialEq)]
struct SkillName(Arc<str>);
impl Display for SkillName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, PartialEq)]
enum PromptElement {
    PromptContent(Arc<str>),
    ToolRef { ns: ToolNamespace, name: ToolName },
    SkillRef { name: SkillName },
    IncludeRef { path: PathBuf },
}
impl Display for PromptElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PromptContent(c) => c.fmt(f),
            Self::ToolRef { ns, name } => {
                f.write_char('`')?;
                ns.fmt(f)?;
                f.write_str("__")?;
                name.fmt(f)?;
                f.write_char('`')
            }
            Self::SkillRef { name } => name.fmt(f),
            Self::IncludeRef { path } => {
                let content = fs::read_to_string(&path).expect("should be able to read the file");
                f.write_str(&content)
            }
        }
    }
}

/// A comment that vanishes mid-sentence leaves two `PromptContent` runs where the
/// prompt had one. Fold them back together so the element list mirrors the prose.
/// `flatten` is what discards the comments' `None`s.
fn coalesce(parts: Vec<Option<PromptElement>>) -> Vec<PromptElement> {
    let mut out: Vec<PromptElement> = Vec::new();
    for part in parts.into_iter().flatten() {
        if let PromptElement::PromptContent(next) = &part {
            // Borrow ends at the `continue`, so the `push` below stays legal.
            if let Some(PromptElement::PromptContent(prev)) = out.last_mut() {
                *prev = format!("{prev}{next}").into();
                continue;
            }
        }
        out.push(part);
    }
    out
}

peg::parser! {
    grammar prompt_parser() for str {
        // File system path definition. The prompt file path syntax does not support relative
        // parent directory or absolute path syntax to sandbox what the prompt can reference.
        rule inner_name()      = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+
        rule extension()       = ['a'..='z' | 'A'..='Z' | '0'..='9']+
        rule file_name()       = inner_name() ("." extension())?
        rule dir_name()        = inner_name() "/"
        rule path() -> &'input str
                               = path:$(dir_name()+ file_name()? / file_name()) { path }

        // Embedded tool name patterns
        rule literal_char()    = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '/' | '.']
        rule wildcard()        = "*" / "?"
        rule class_match()     = "[" "!"? literal_char()+ "]"
        rule alt_branch()      = (literal_char() / wildcard() / class_match())*
        rule alt_match()       = "{" alt_branch() ++ "," "}"
        rule glob_match()      = wildcard() / class_match() / alt_match()
        rule glob_element()    = literal_char()* glob_match() (literal_char() / glob_match())*
        rule literal_element() = literal_char()*<1,64>
        rule element()         = glob_element() / literal_element()
        rule ns_pattern() -> ToolNamespace
                               = ns:$element() ":" { ToolNamespace(ns.into()) }
        rule name_pattern() -> ToolName
                               = n:$element() { ToolName(n.into()) }
        rule tool_fqn() -> PromptElement
                               = ns:ns_pattern() name:name_pattern() { PromptElement::ToolRef { ns, name } }

        // Structured prompt commands
        rule sigil()           = "@"
        rule tool_ref() -> PromptElement
                               = sigil() "`" t:tool_fqn() "`" { t }
        rule include_ref() -> PromptElement
                               = sigil() "{" p:path() "}" { PromptElement::IncludeRef { path: p.into() } }
        rule skill_ref() -> PromptElement
                               = sigil() "(" name:path() ")" { PromptElement::SkillRef { name: SkillName(name.into()) } }
        rule prompt_commands() -> PromptElement
                               = tool_ref() / skill_ref() / include_ref()

        // Comments do not nest, so the body stops at the first closer. comment_open() guards
        // content while comment() consumes it; the gap between the two is deliberate, and is
        // what makes an unterminated comment an error instead of prose. Keep them in sync.
        rule comment_open()    = "<!--"
        rule comment()         = comment_open() (!"-->" [_])* "-->"

        // A comment inside command delimiters is an error, not prose: refusing the span
        // leaves the position unconsumable. Requiring the closer keeps a stray `@{` from
        // tripping it; !comment_open() stops peg's greedy `*` from scanning past the comment.
        rule commented_command()
                               = sigil() "`" (!("`" / "\n") !comment_open() [_])* comment_open() (!("`" / "\n") [_])* "`"
                               / sigil() "(" (!(")" / "\n") !comment_open() [_])* comment_open() (!(")" / "\n") [_])* ")"
                               / sigil() "{" (!("}" / "\n") !comment_open() [_])* comment_open() (!("}" / "\n") [_])* "}"

        // prompt literal content
        rule code_block_fence()
                               = "```" (!"```" [_])* "```"
        rule content_char()    = code_block_fence()
                               / (!prompt_commands() !comment_open() !commented_command() [_])
        rule content_literal() -> PromptElement
                               = c:$(content_char()+) { PromptElement::PromptContent(c.into()) }

        // A comment parses to None: consumed from the stream, never emitted. Everything
        // else carries a value through, and coalesce() drops the Nones.
        rule prompt_part() -> Option<PromptElement>
                               = comment() { None }
                               / e:(prompt_commands() / content_literal()) { Some(e) }

        // refs must come first: content_literal() would otherwise always win.
        pub rule prompts() -> Vec<PromptElement>
                               = parts:prompt_part()* { coalesce(parts) }
    }
}

/// Renders a unified diff with GitHub-style intra-line highlighting: changed lines are
/// tinted, and the specific words that differ within them are emphasized. `console` drops
/// the escape codes automatically when stdout is not a terminal.
fn render_inline_diff(old: &str, new: &str) -> String {
    // Words is GitHub's granularity. Semantic cleanup shifts highlight boundaries to more
    // readable token edges; it is off by default.
    let mut opts = InlineChangeOptions::new();
    opts.mode(InlineChangeMode::Words).semantic_cleanup(true);

    let diff = TextDiff::from_lines(old, new);

    let ctx = Style::new().dim();
    let del = Style::new().red();
    let ins = Style::new().green();
    let del_hl = Style::new().red().bold().underlined();
    let ins_hl = Style::new().green().bold().underlined();

    let mut out = String::new();
    let mut ud = diff.unified_diff();
    ud.context_radius(1);

    for (idx, hunk) in ud.iter_hunks().enumerate() {
        if idx == 0 {
            writeln!(out, "--- original").unwrap();
            writeln!(out, "+++ transpiled").unwrap();
        }
        writeln!(out, "{}", ctx.apply_to(hunk.header())).unwrap();

        for op in hunk.ops() {
            for change in diff.iter_inline_changes_with_options(op, opts) {
                let (marker, line, hl) = match change.tag() {
                    ChangeTag::Delete => ("-", &del, &del_hl),
                    ChangeTag::Insert => ("+", &ins, &ins_hl),
                    ChangeTag::Equal => (" ", &ctx, &ctx),
                };
                write!(out, "{}", line.apply_to(marker)).unwrap();
                for (emphasized, value) in change.iter_strings_lossy() {
                    let style = if emphasized { hl } else { line };
                    write!(out, "{}", style.apply_to(value)).unwrap();
                }
                // from_lines keeps the trailing newline in the value; only a final line
                // that lacks one needs it added back.
                if change.missing_newline() {
                    out.push('\n');
                }
            }
        }
    }
    out
}

const EXAMPLE_PROMPT: &'static str = r#"
You are a site reliability engineering agent. The goal of our SRE team is to ensure quick
resolution to system issues with minimal downtime. Our stack includes:

<!--
    the list of services are maintained by a cron script. instead of splicing them into each
    prompt, we can just use the include syntax to bring them into the prompt script.
-->
@{sample/snippets/service_def.md}

When inspecting the business services, you should be able to use the @`mezmo_internal:describe` tool in order to
find out service specific details. Always include a kubernetes namespace on all @`k8s:*` tools. Fall back to
the `find_service` tool if @`mezmo_internal:describe` returns no results.

```
<!-- this isn't a comment since it's in a code block -->
<h1>Code Block</h1>
```

`not_a_tool:just_documentation` <!-- another comment -->

Generate a report using @(skills/audit-report.md) of deployed node.js services that have a trace calling the
`internal_mezmo_auth` function in the `mezmo_auth` package. You might also need to look at deployed infra for sidecar
applications that use the `mezmo_auth` package. Group the report into sections by deployment environment. Send
the report to `ops@mezmo.com`.
"#;

/// Parses a prompt and reports the tool and skill references embedded in it.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Prompt file to parse, or `-` for stdin. Omit to use the built-in example.
    prompt_file: Option<PathBuf>,
}

/// Reads the prompt the user selected, or falls back to the example.
/// `Err` holds a message already formatted for the user.
fn load_prompt(prompt_file: Option<PathBuf>) -> Result<String, String> {
    let Some(path) = prompt_file else {
        return Ok(EXAMPLE_PROMPT.to_string());
    };

    // `-` is the conventional spelling for stdin, and makes the tool pipeable.
    if path.as_os_str() == "-" {
        let mut buf = String::new();
        return match std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf) {
            Ok(_) => Ok(buf),
            Err(e) => Err(format!("reading stdin: {e}")),
        };
    }

    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text),
        Err(e) => Err(format!("reading {}: {e}", path.display())),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let prompt = match load_prompt(cli.prompt_file) {
        Ok(loaded) => loaded,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::FAILURE;
        }
    };

    let res = match prompt_parser::prompts(&prompt) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut content = Vec::new();
    let mut tools = Vec::new();
    let mut skills = Vec::new();
    let mut includes = Vec::new();
    let mut xformed_prompt = String::new();

    for element in &res {
        xformed_prompt.push_str(&element.to_string());
        match element {
            PromptElement::PromptContent(s) => content.push(s),
            PromptElement::ToolRef { ns, name } => {
                tools.push(format!("ns={}, name={}", ns.0.as_ref(), name.0.as_ref()))
            }
            PromptElement::SkillRef { name } => skills.push(name.0.as_ref()),
            PromptElement::IncludeRef { path } => {
                includes.push(path.to_str().expect("only use ascii paths"))
            }
        }
    }

    let diff = render_inline_diff(&prompt, &xformed_prompt);

    println!(
        r#"
Input Prompt
------------
{prompt}

PromptElement::PromptContent
----------------------------
{content:#?}

PromptElement::ToolRef
----------------------
{tools:#?}

PromptElement::SkillRef
-----------------------
{skills:#?}

PromptElement::IncludeRef
-------------------------
{includes:#?}

Transpiled Prompt
-----------------
{xformed_prompt}

Prompt Diff
-----------
{diff}
"#
    );

    ExitCode::SUCCESS
}
