//-------------------------------------------------------------------------------------------
// The verbose grammar parser and example prompt
//-------------------------------------------------------------------------------------------
use crate::ast::{PromptElement, coalesce};

peg::parser! {
    pub grammar prompt_parser() for str {
        // File system path definition. The prompt file path syntax does not support relative
        // parent directory, absolute path, or directory syntax to sandbox what the prompt can
        // reference.
        rule name()            = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+
        rule extension()       = ['a'..='z' | 'A'..='Z' | '0'..='9']+
        rule path() -> &'input str
                               = $(name() ++ "/" ("." extension())?)

        // Embedded tool name patterns. Literals and glob syntax match in a single pass.
        rule literal_char()    = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '/' | '.']
        rule wildcard()        = "*" / "?"
        rule class_match()     = "[" "!"? literal_char()+ "]"
        rule alt_branch()      = (literal_char() / wildcard() / class_match())*
        rule alt_match()       = "{" alt_branch() ++ "," "}"
        rule pattern() -> &'input str
                               = $((literal_char() / wildcard() / class_match() / alt_match())+)
        rule tool_fqn() -> PromptElement
                               = ns:pattern() ":" name:pattern() { PromptElement::new_tool(ns, name) }

        // Structured prompt commands: `@`, a keyword, and a parenthesized argument.
        rule command() -> PromptElement
                               = "@" c:( "Tool("    t:tool_fqn() ")" { t }
                                       / "Skill("   p:path()     ")" { PromptElement::new_skill(p) }
                                       / "Include(" p:path()     ")" { PromptElement::new_include(p.into()) } )
                                     { c }

        // Comments do not nest, so the body stops at the first closer. comment_open() guards
        // content while comment() consumes it; the gap between the two is deliberate, and is
        // what makes an unterminated comment an error instead of prose. Keep them in sync.
        rule comment_open()    = "<!--"
        rule comment()         = comment_open() (!"-->" [_])* "-->"

        // A comment inside a command is an error, not prose: refusing the span leaves the
        // position unconsumable. Requiring the `)` keeps a stray `@Tool(` from tripping it;
        // !comment_open() stops peg's greedy `*` from scanning past the comment.
        rule command_open()    = "@" ("Tool(" / "Skill(" / "Include(")
        rule commented_command()
                               = command_open() (!(")" / "\n") !comment_open() [_])* comment_open() (!(")" / "\n") [_])* ")"

        // prompt literal content. text() takes runs that can't start a command, comment, or
        // fence in one step, so the guards only run at `@`, `<`, and backtick.
        rule code_block_fence()
                               = "```" (!"```" [_])* "```"
        rule text()            = [^ '@' | '<' | '`']+
        rule content_char()    = text() / code_block_fence()
                               / (!command() !comment_open() !commented_command() [_])
        rule content_literal() -> PromptElement
                               = c:$(content_char()+) { PromptElement::new_content(c) }

        // A comment parses to None: consumed from the stream, never emitted. Everything
        // else carries a value through, and coalesce() drops the Nones.
        rule prompt_part() -> Option<PromptElement>
                               = comment() { None }
                               / e:(command() / content_literal()) { Some(e) }

        // commands must come first: content_literal() would otherwise always win.
        pub rule prompts() -> Vec<PromptElement>
                               = parts:prompt_part()* { coalesce(parts) }
    }
}

pub const EXAMPLE_PROMPT: &'static str = r#"
You are a site reliability engineering agent. The goal of our SRE team is to ensure quick
resolution to system issues with minimal downtime. Our stack includes:

<!--
    the list of services are maintained by a cron script. instead of splicing them into each
    prompt, we can just use the include syntax to bring them into the prompt script.
-->
@Include(sample/snippets/service_def.md)

When inspecting the business services, you should be able to use the @Tool(mezmo_internal:describe) tool in order to
find out service specific details. Always include a kubernetes namespace on all @Tool(k8s:*) tools. Fall back to
the @Tool(`find_service`) tool if @Tool(mezmo_internal:describe) returns no results.

```
<!-- this isn't a comment since it's in a code block -->
<h1>Code Block</h1>
```

`not_a_tool:just_documentation` <!-- another comment -->

Generate a report using @Skill(skills/audit-report.md) of deployed node.js services that have a trace calling the
`internal_mezmo_auth` function in the `mezmo_auth` package. You might also need to look at deployed infra for sidecar
applications that use the `mezmo_auth` package. Group the report into sections by deployment environment. Send
the report to `ops@mezmo.com`.
"#;
