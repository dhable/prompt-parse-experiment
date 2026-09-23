use clap::Parser;
use console::Style;
use similar::{ChangeTag, InlineChangeMode, InlineChangeOptions, TextDiff};
use std::fmt::{Display, Write};
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
        }
    }
}

peg::parser! {
    grammar prompt_parser() for str {
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

        rule ns_pattern() -> ToolNamespace = ns:$element() ":" { ToolNamespace(ns.into()) }
        rule name_pattern() -> ToolName = n:$element() { ToolName(n.into()) }
        rule tool_fqn() -> PromptElement
            =  ns:ns_pattern() name:name_pattern() { PromptElement::ToolRef { ns, name }
        }

        rule tool_ref_start() = "@`"
        rule tool_ref() -> PromptElement = tool_ref_start() t:tool_fqn() "`" { t }

        // Embedded skill patterns
        rule skill_path() -> PromptElement
            = name:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '/' | '.' | '_' | '-']*) {
                PromptElement::SkillRef { name: SkillName(name.into()) }
            }
        rule skill_ref_start() = "@("
        rule skill_ref() -> PromptElement = skill_ref_start() s:skill_path() ")" { s }


        // prompt literal content
        rule ref_start() = tool_ref_start() / skill_ref_start()
        rule code_block_fence() = "```" (!"```" [_])* "```"
        rule content_char() = code_block_fence() / (!ref_start() [_])
        rule content_literal() -> PromptElement
            = c:$(content_char()+) { PromptElement::PromptContent(c.into()) }

        // refs must come first: content_literal() would otherwise always win.
        pub rule prompts() -> Vec<PromptElement>
            = (tool_ref() / skill_ref() / content_literal())*
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

    * Kubernetes
    * Postgres
    * MongoDB
    * node.js business services
    * rust business services
    * Apache Pulsar

When inspecting the business services, you should be able to use the @`mezmo_internal:describe` tool in order to
find out service specific details. Always include a kubernetes namespace on all @`k8s:*` tools. Fall back to
the `find_service` tool if @`mezmo_internal:describe` returns no results.

```not_a_tool:just_a_block```

`not_a_tool:just_documentation`

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
    let mut xformed_prompt = String::new();

    for element in &res {
        xformed_prompt.push_str(&element.to_string());
        match element {
            PromptElement::PromptContent(s) => content.push(s),
            PromptElement::ToolRef { ns, name } => {
                tools.push(format!("ns={}, name={}", ns.0.as_ref(), name.0.as_ref()))
            }
            PromptElement::SkillRef { name } => skills.push(name.0.as_ref()),
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
{skills:#?}

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
