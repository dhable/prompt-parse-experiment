use clap::{Parser, ValueEnum};
use console::Style;
use similar::{ChangeTag, InlineChangeMode, InlineChangeOptions, TextDiff};
use std::fmt::Write;
use std::path::PathBuf;
use std::process::ExitCode;

mod ast;
mod terse;
mod verbose;

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

/// Which grammar to parse the prompt with. Each lives in its own module with a
/// `prompt_parser` grammar and an `EXAMPLE_PROMPT` written in its syntax.
#[derive(Clone, Copy, ValueEnum)]
enum ParserKind {
    Terse,
    Verbose,
}
impl ParserKind {
    fn parse(
        self,
        prompt: &str,
    ) -> Result<Vec<ast::PromptElement>, peg::error::ParseError<peg::str::LineCol>> {
        match self {
            Self::Terse => terse::prompt_parser::prompts(prompt),
            Self::Verbose => verbose::prompt_parser::prompts(prompt),
        }
    }

    // The examples are written in their own grammar's syntax, so the fallback has to
    // follow the parser rather than always being the terse one.
    fn example_prompt(self) -> &'static str {
        match self {
            Self::Terse => terse::EXAMPLE_PROMPT,
            Self::Verbose => verbose::EXAMPLE_PROMPT,
        }
    }
}

/// Parses a prompt and reports the tool and skill references embedded in it.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Grammar to parse the prompt with.
    #[arg(long, value_enum)]
    parser: ParserKind,

    /// File to read the prompt from. Omit along with --stdio to use the example prompt.
    #[arg(long, conflicts_with = "stdio")]
    prompt_file: Option<PathBuf>,

    /// Read the prompt from stdin.
    #[arg(long)]
    stdio: bool,
}

/// Reads the prompt the user selected, or falls back to the parser's example.
/// `Err` holds a message already formatted for the user.
fn load_prompt(cli: &Cli) -> Result<String, String> {
    if cli.stdio {
        let mut buf = String::new();
        return match std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf) {
            Ok(_) => Ok(buf),
            Err(e) => Err(format!("reading stdin: {e}")),
        };
    }

    if let Some(path) = &cli.prompt_file {
        return std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()));
    }

    Ok(cli.parser.example_prompt().to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let prompt = match load_prompt(&cli) {
        Ok(loaded) => loaded,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::FAILURE;
        }
    };

    let res = match cli.parser.parse(&prompt) {
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
            ast::PromptElement::PromptContent(s) => content.push(s),
            ast::PromptElement::ToolRef { ns, name } => {
                tools.push(format!("ns={}, name={}", ns.0.as_ref(), name.0.as_ref()))
            }
            ast::PromptElement::SkillRef { name } => skills.push(name.0.as_ref()),
            ast::PromptElement::IncludeRef { path } => {
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
